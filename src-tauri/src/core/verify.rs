//! Parallel SHA-1 piece verification of client files against a torrent.

use super::torrent::Torrent;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Hashes `pieces` on disk and returns the ones that don't match (missing or
/// short files count as mismatches). `progress(bytes_done, bytes_total)` is
/// called from worker threads.
pub fn verify_pieces(
    t: &Torrent,
    root: &Path,
    pieces: &[usize],
    progress: &(dyn Fn(u64, u64) + Sync),
    cancel: &AtomicBool,
) -> Vec<usize> {
    let total: u64 = pieces.iter().map(|&p| t.piece_span(p).end - t.piece_span(p).start).sum();
    let done = AtomicU64::new(0);
    let next = AtomicUsize::new(0);
    let failed = Mutex::new(Vec::new());
    // Disk-bound past a few threads; more just thrashes HDDs.
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 6);

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                let mut handles: HashMap<usize, Option<File>> = HashMap::new();
                let mut buf = vec![0u8; t.piece_length as usize];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&piece) = pieces.get(i) else { return };
                    let span = t.piece_span(piece);
                    let len = (span.end - span.start) as usize;
                    if !read_span(t, root, span.clone(), &mut buf[..len], &mut handles) || Sha1::digest(&buf[..len]).as_slice() != t.pieces[piece] {
                        failed.lock().unwrap().push(piece);
                    }
                    progress(done.fetch_add(len as u64, Ordering::Relaxed) + len as u64, total);
                }
            });
        }
    });

    let mut failed = failed.into_inner().unwrap();
    failed.sort_unstable();
    failed
}

fn read_span(t: &Torrent, root: &Path, span: std::ops::Range<u64>, out: &mut [u8], handles: &mut HashMap<usize, Option<File>>) -> bool {
    let mut written = 0usize;
    for fi in t.files_in(span.clone()) {
        let f = &t.files[fi];
        let start = span.start.max(f.offset);
        let end = span.end.min(f.offset + f.length);
        let n = (end - start) as usize;
        let Some(file) = handles.entry(fi).or_insert_with(|| File::open(t.local_path(root, fi)).ok()) else { return false };
        if file.seek(SeekFrom::Start(start - f.offset)).is_err() || file.read_exact(&mut out[written..written + n]).is_err() {
            return false;
        }
        written += n;
    }
    written == out.len()
}

/// Maps failed pieces to the files that need re-downloading. A failed piece that
/// straddles files is blamed on the ones already known to be bad when possible,
/// so a good neighbour isn't re-downloaded for nothing.
pub fn blame(t: &Torrent, failed: &[usize], known_bad: &HashSet<usize>) -> HashSet<usize> {
    let mut bad = HashSet::new();
    for &p in failed {
        let files: Vec<usize> = t.files_in(t.piece_span(p)).collect();
        let culprits: Vec<usize> = files.iter().copied().filter(|f| known_bad.contains(f)).collect();
        bad.extend(if culprits.is_empty() { files } else { culprits });
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_corrupt_and_missing_files() {
        let dir = std::env::temp_dir().join(format!("octo-verify-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Data")).unwrap();
        let files: [(&str, &[u8]); 3] = [("Data/a.mpq", b"0123456789"), ("b.dll", b"abcdef"), ("c.txt", b"zz")];
        let plen = 4usize;
        let data: Vec<u8> = files.iter().flat_map(|(_, c)| c.iter().copied()).collect();
        let pieces: Vec<u8> = data.chunks(plen).flat_map(|c| Sha1::digest(c).to_vec()).collect();
        let mut info = b"d5:filesl".to_vec();
        for (p, c) in &files {
            let parts: String = p.split('/').map(|s| format!("{}:{}", s.len(), s)).collect();
            info.extend(format!("d6:lengthi{}e4:pathl{}ee", c.len(), parts).as_bytes());
        }
        info.extend(format!("e4:name6:client12:piece lengthi{plen}e6:pieces{}:", pieces.len()).as_bytes());
        info.extend(&pieces);
        let mut bytes = b"d4:info".to_vec();
        bytes.extend(info);
        bytes.extend(b"ee");
        let t = Torrent::parse(&bytes).unwrap();

        std::fs::write(dir.join("Data/a.mpq"), b"0123456789").unwrap();
        std::fs::write(dir.join("b.dll"), b"abcXef").unwrap(); // corrupt
        // c.txt missing

        let all: Vec<usize> = (0..t.pieces.len()).collect();
        let failed = verify_pieces(&t, &dir, &all, &|_, _| {}, &AtomicBool::new(false));
        let bad = blame(&t, &failed, &HashSet::from([2]));
        assert!(!bad.contains(&0), "a.mpq is intact");
        assert!(bad.contains(&1) && bad.contains(&2));
        std::fs::remove_dir_all(dir).ok();
    }
}
