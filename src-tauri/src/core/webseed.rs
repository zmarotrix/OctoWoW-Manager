//! Downloads torrent content straight from the torrent's HTTP web seed
//! (BEP 19 `url-list`) with parallel range requests. Every piece is checked
//! against the torrent's SHA-1 before it's written, so a bad mirror or a
//! flipped bit can't corrupt the client.
//!
//! Only bytes belonging to the selected files are written; neighbouring files
//! that share a piece are read from the network for hashing but never touched.

use super::torrent::Torrent;
use sha1::{Digest, Sha1};
use std::collections::{HashSet, VecDeque};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Parallel connections to the mirror.
const WORKERS: usize = 6;
/// Pieces per request (1 MiB pieces → 8 MiB ranges).
const CHUNK_PIECES: usize = 8;
const RETRIES: u32 = 5;

#[derive(Clone, Debug)]
pub struct Progress {
    /// Per selected file (same order as `selected`): bytes verified and written.
    pub done: Vec<u64>,
    /// Bytes per second over the network, smoothed.
    pub speed: u64,
    pub active: u32,
}

struct Shared {
    t: Torrent,
    root: PathBuf,
    base_url: String,
    selected: Vec<usize>,
    queue: Mutex<VecDeque<Vec<usize>>>,
    done_pieces: Mutex<HashSet<usize>>,
    file_done: Vec<AtomicU64>,
    net_bytes: AtomicU64,
    active: AtomicUsize,
    failed: AtomicBool,
    http: reqwest::Client,
}

/// Percent-encodes one URL path segment.
fn enc(seg: &str) -> String {
    seg.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// BEP 19: for multi-file torrents the file URL is `<url-list>/<name>/<path>`.
pub fn file_url(base: &str, t: &Torrent, file: usize) -> String {
    let base = base.trim_end_matches('/');
    let path: Vec<String> = t.files[file].path.split('/').map(enc).collect();
    format!("{base}/{}/{}", enc(&t.name), path.join("/"))
}

/// Bytes of `piece` that belong to `file`.
fn overlap(t: &Torrent, piece: usize, file: usize) -> u64 {
    let span = t.piece_span(piece);
    let f = &t.files[file];
    span.end.min(f.offset + f.length).saturating_sub(span.start.max(f.offset))
}

/// Fetches `[start, end)` of the torrent's concatenated data from the mirror.
async fn fetch_span(sh: &Shared, start: u64, end: u64, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity((end - start) as usize);
    for fi in sh.t.files_in(start..end) {
        let f = &sh.t.files[fi];
        let (from, to) = (start.max(f.offset) - f.offset, end.min(f.offset + f.length) - f.offset);
        let url = file_url(&sh.base_url, &sh.t, fi);
        let mut attempt = 0;
        loop {
            if cancel.load(Ordering::Relaxed) || sh.failed.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let before = buf.len();
            match fetch_range(sh, &url, from, to, &mut buf, cancel).await {
                Ok(()) => break,
                Err(e) => {
                    buf.truncate(before);
                    attempt += 1;
                    if attempt >= RETRIES || cancel.load(Ordering::Relaxed) {
                        return Err(format!("{}: {e}", f.path));
                    }
                    tokio::time::sleep(Duration::from_secs(1 << attempt.min(4))).await;
                }
            }
        }
    }
    Ok(buf)
}

async fn fetch_range(sh: &Shared, url: &str, from: u64, to: u64, buf: &mut Vec<u8>, cancel: &AtomicBool) -> Result<(), String> {
    let mut res = sh.http.get(url).header(reqwest::header::RANGE, format!("bytes={from}-{}", to - 1)).send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    let whole_file = status == reqwest::StatusCode::OK && from == 0;
    if status != reqwest::StatusCode::PARTIAL_CONTENT && !whole_file {
        return Err(format!("mirror answered HTTP {status}"));
    }
    let want = (to - from) as usize;
    let start = buf.len();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        sh.net_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        let room = want - (buf.len() - start);
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if buf.len() - start == want {
            break;
        }
    }
    if buf.len() - start != want {
        return Err("connection closed early".into());
    }
    Ok(())
}

/// Writes the selected files' share of a verified span to disk.
fn write_span(sh: &Shared, start: u64, data: &[u8]) -> Result<(), String> {
    let end = start + data.len() as u64;
    for (k, &fi) in sh.selected.iter().enumerate() {
        let f = &sh.t.files[fi];
        let (a, b) = (start.max(f.offset), end.min(f.offset + f.length));
        if a >= b {
            continue;
        }
        let mut file = std::fs::OpenOptions::new().write(true).open(sh.t.local_path(&sh.root, fi)).map_err(|e| format!("Couldn't write {}: {e}", f.path))?;
        file.seek(SeekFrom::Start(a - f.offset)).and_then(|_| file.write_all(&data[(a - start) as usize..(b - start) as usize])).map_err(|e| format!("Couldn't write {}: {e}", f.path))?;
        sh.file_done[k].fetch_add(b - a, Ordering::Relaxed);
    }
    Ok(())
}

async fn worker(sh: Arc<Shared>, cancel: Arc<AtomicBool>) -> Result<(), String> {
    loop {
        let Some(run) = sh.queue.lock().unwrap().pop_front() else { return Ok(()) };
        sh.active.fetch_add(1, Ordering::Relaxed);
        let result: Result<(), String> = async {
            let (start, end) = (sh.t.piece_span(run[0]).start, sh.t.piece_span(*run.last().unwrap()).end);
            let mut tries = 0;
            loop {
                let data = fetch_span(&sh, start, end, &cancel).await?;
                let bad = run.iter().find(|&&p| {
                    let s = sh.t.piece_span(p);
                    Sha1::digest(&data[(s.start - start) as usize..(s.end - start) as usize]).as_slice() != sh.t.pieces[p]
                });
                match bad {
                    None => {
                        let (sh2, data) = (sh.clone(), data);
                        tokio::task::spawn_blocking(move || write_span(&sh2, start, &data)).await.map_err(|e| e.to_string())??;
                        sh.done_pieces.lock().unwrap().extend(run.iter().copied());
                        return Ok(());
                    }
                    Some(_) if tries < 2 => tries += 1,
                    Some(p) => return Err(format!("Piece {p} failed verification three times; the mirror may be serving a different release.")),
                }
            }
        }
        .await;
        sh.active.fetch_sub(1, Ordering::Relaxed);
        if let Err(e) = result {
            sh.failed.store(true, Ordering::Relaxed);
            return Err(e);
        }
    }
}

/// Downloads `selected` files into `root`. `already` = pieces known to be on disk
/// already (resume). `on_progress` runs every ~500 ms; `on_checkpoint` receives
/// the set of finished pieces every few seconds and at the end, for resuming.
pub async fn download(
    t: &Torrent,
    root: &Path,
    selected: &[usize],
    already: HashSet<usize>,
    cancel: Arc<AtomicBool>,
    on_progress: &(dyn Fn(Progress) + Sync),
    on_checkpoint: &(dyn Fn(&HashSet<usize>) + Sync),
) -> Result<(), String> {
    let base_url = t.web_seeds.first().cloned().ok_or("This release has no HTTP mirror (web seed) to download from.")?;

    // Pre-size every selected file so ranges can be written in any order.
    for &fi in selected {
        let path = t.local_path(root, fi);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
        }
        let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path).map_err(|e| format!("Couldn't open {} (is the game running?): {e}", t.files[fi].path))?;
        if f.metadata().map(|m| m.len()).ok() != Some(t.files[fi].length) {
            f.set_len(t.files[fi].length).map_err(|e| e.to_string())?;
        }
    }

    let mut needed: Vec<usize> = selected.iter().flat_map(|&f| t.pieces_for(f)).filter(|p| !already.contains(p)).collect();
    needed.sort_unstable();
    needed.dedup();
    let mut queue: VecDeque<Vec<usize>> = VecDeque::new();
    for p in needed {
        match queue.back_mut() {
            Some(run) if run.len() < CHUNK_PIECES && *run.last().unwrap() + 1 == p => run.push(p),
            _ => queue.push_back(vec![p]),
        }
    }

    let file_done: Vec<AtomicU64> = selected.iter().map(|&fi| AtomicU64::new(already.iter().map(|&p| overlap(t, p, fi)).sum())).collect();
    let sh = Arc::new(Shared {
        t: t.clone(),
        root: root.to_path_buf(),
        base_url,
        selected: selected.to_vec(),
        queue: Mutex::new(queue),
        done_pieces: Mutex::new(already),
        file_done,
        net_bytes: AtomicU64::new(0),
        active: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
        http: reqwest::Client::builder()
            .user_agent(concat!("OctoWoW-Manager/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?,
    });

    let handles: Vec<_> = (0..WORKERS).map(|_| tokio::spawn(worker(sh.clone(), cancel.clone()))).collect();

    let report = |speed: u64| on_progress(Progress { done: sh.file_done.iter().map(|d| d.load(Ordering::Relaxed)).collect(), speed, active: sh.active.load(Ordering::Relaxed) as u32 });
    let (mut last_bytes, mut last_tick, mut last_checkpoint, mut speed) = (0u64, Instant::now(), Instant::now(), 0f64);
    while handles.iter().any(|h| !h.is_finished()) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let bytes = sh.net_bytes.load(Ordering::Relaxed);
        let dt = last_tick.elapsed().as_secs_f64().max(0.001);
        let instant = (bytes - last_bytes) as f64 / dt;
        speed = if speed == 0.0 { instant } else { speed * 0.7 + instant * 0.3 };
        (last_bytes, last_tick) = (bytes, Instant::now());
        report(speed as u64);
        if last_checkpoint.elapsed() > Duration::from_secs(3) {
            on_checkpoint(&sh.done_pieces.lock().unwrap());
            last_checkpoint = Instant::now();
        }
    }

    let mut first_err = None;
    for h in handles {
        if let Err(e) = h.await.map_err(|e| e.to_string()).and_then(|r| r) {
            if e != "cancelled" && first_err.is_none() {
                first_err = Some(e);
            }
        }
    }
    on_checkpoint(&sh.done_pieces.lock().unwrap());
    report(0);
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled. Progress is kept and resumes next time.".into());
    }
    first_err.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_bep19_urls() {
        let bytes = b"d4:infod5:filesld6:lengthi3e4:pathl4:Data9:a b+c.mpqeee4:name6:client12:piece lengthi4e6:pieces20:aaaaaaaaaaaaaaaaaaaaee";
        let t = Torrent::parse(bytes).unwrap();
        assert_eq!(file_url("https://dl.octowow.st/client/", &t, 0), "https://dl.octowow.st/client/client/Data/a%20b%2Bc.mpq");
        assert_eq!(file_url("https://dl.octowow.st/client", &t, 0), "https://dl.octowow.st/client/client/Data/a%20b%2Bc.mpq");
    }
}
