//! Client file sync against the official OctoWoW torrent.
//!
//! * **Check**: fetch the .torrent once, compare file sizes on disk, and diff
//!   piece hashes against the last synced release to find files that changed
//!   without changing size. Only ambiguous pieces get hashed locally.
//! * **Verify**: hash every piece on disk (parallel) to find corruption.
//! * **Update**: download just the files that need it from the release's HTTPS
//!   mirror (web seed), verifying every piece against the torrent's SHA-1
//!   hashes; files owned by Client Tweaks are never touched.

use super::torrent::{diff, Delta, Torrent};
use super::verify::{blame, verify_pieces};
use serde::{Deserialize, Serialize};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TORRENT_URL: &str = "https://dl.octowow.st/download/client.torrent";

/// Stock runtime DLLs that ship with the client and aren't plugins.
const RUNTIME_DLLS: [&str; 5] = ["divxdecoder.dll", "dbghelp.dll", "fmod.dll", "ijl15.dll", "unicows.dll"];

/// Archives older clients shipped under names the current one no longer uses.
/// Matched by name *and* exact size so player mods reusing a name are safe.
const LEGACY_ARCHIVES: [(&str, u64); 5] = [
    ("patch-6.mpq", 451_195_806),
    ("patch-7.mpq", 175_256_564),
    ("patch-8.mpq", 484_649_870),
    ("patch-9.mpq", 506_808_141),
    ("patch-a.mpq", 241_751_337),
];

#[derive(Clone, Copy, PartialEq)]
enum Owner {
    Client,
    /// Plugin DLLs / DXVK: Client Tweaks installs, replaces and removes these;
    /// the updater never downloads, checks or reports them.
    Tweaks,
    /// Small config the player may edit (realmlist.wtf).
    User,
}

fn owner(path: &str) -> Owner {
    let p = path.to_ascii_lowercase();
    if p == "realmlist.wtf" {
        Owner::User
    } else if !p.contains('/') && ((p.ends_with(".dll") && !RUNTIME_DLLS.contains(&p.as_str())) || p == "vanillafixes.exe") {
        Owner::Tweaks
    } else {
        Owner::Client
    }
}

// ---------- report types (sent to the UI) ----------

#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum FileState {
    Ok,
    Missing,
    SizeMismatch,
    /// Changed in the new release.
    Outdated,
    /// Hash check failed with no release change to explain it.
    Corrupt,
    /// Owned by Client Tweaks; left alone.
    Managed,
    /// Player-edited config; left alone.
    UserEdited,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReport {
    pub path: String,
    pub size: u64,
    pub local_size: Option<u64>,
    pub state: FileState,
    pub detail: String,
    pub download: bool,
    /// New in this release, compared with the last sync.
    pub is_new: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub info_hash: String,
    pub files: usize,
    pub total_size: u64,
    pub created: Option<i64>,
    pub published: Option<String>,
    pub web_seed: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub wow_dir: String,
    pub info_hash: String,
    pub synced_at: u64,
    pub verified: bool,
}

#[derive(Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PlanStatus {
    NotInstalled,
    UpToDate,
    UpdateAvailable,
    RepairNeeded,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Obsolete {
    pub path: String,
    pub size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub status: PlanStatus,
    pub checked_at: u64,
    pub release: ReleaseInfo,
    pub baseline: Option<Baseline>,
    pub release_changed: bool,
    /// Every file was hash-checked this time.
    pub deep_verified: bool,
    /// No previous sync to diff against: same-size changes can't be detected
    /// without a full verify.
    pub size_only: bool,
    pub files: Vec<FileReport>,
    pub download_files: usize,
    pub download_bytes: u64,
    pub obsolete: Vec<Obsolete>,
    /// In the last synced release but not this one (left on disk).
    pub dropped: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub level: &'static str,
    pub text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
    pub path: String,
    pub size: u64,
    pub done: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub phase: &'static str,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed: u64,
    /// Parallel connections to the mirror currently transferring.
    pub connections: u32,
    pub files: Vec<FileProgress>,
}

// ---------- persistence ----------

#[derive(Serialize, Deserialize)]
struct ActiveDownload {
    wow_dir: String,
    info_hash: String,
    /// Pieces already verified and written (for resuming).
    #[serde(default)]
    done_pieces: Vec<usize>,
}

struct Cached {
    etag: Option<String>,
    last_modified: Option<String>,
    bytes: Arc<Vec<u8>>,
}

pub struct Updater {
    dir: PathBuf,
    http: reqwest::Client,
    cache: Mutex<Option<Cached>>,
    pub plan: Mutex<Option<Plan>>,
    pub busy: AtomicBool,
    pub cancel: Arc<AtomicBool>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    std::fs::write(path, serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Sizes in log lines, e.g. "1.9 GB".
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

/// "1.9 GB → 2.1 GB", or "+58.2 MB" when both round to the same size.
fn size_change(old: u64, new: u64) -> String {
    let (a, b) = (human(old), human(new));
    if a != b {
        return format!("{a} → {b}");
    }
    let sign = if new >= old { '+' } else { '−' };
    format!("{sign}{}", human(new.abs_diff(old)))
}

impl Updater {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            dir: data_dir.join("client"),
            http: reqwest::Client::builder()
                .user_agent(concat!("OctoWoW-Manager/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("http client"),
            cache: Mutex::new(None),
            plan: Mutex::new(None),
            busy: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Downloads the .torrent, reusing the cached copy when the server says it's unchanged.
    pub async fn fetch_release(&self) -> Result<(Arc<Vec<u8>>, Option<String>), String> {
        let (etag, modified) = {
            let c = self.cache.lock().unwrap();
            (c.as_ref().and_then(|c| c.etag.clone()), c.as_ref().and_then(|c| c.last_modified.clone()))
        };
        let mut req = self.http.get(TORRENT_URL);
        if let Some(e) = &etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, e);
        }
        let res = req.send().await.map_err(|e| format!("Couldn't reach the OctoWoW servers: {e}"))?;
        if res.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(c) = self.cache.lock().unwrap().as_ref() {
                return Ok((c.bytes.clone(), modified));
            }
        }
        if !res.status().is_success() {
            return Err(format!("The release server answered HTTP {}", res.status()));
        }
        let header = |n: reqwest::header::HeaderName| res.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (etag, last_modified) = (header(reqwest::header::ETAG), header(reqwest::header::LAST_MODIFIED));
        let bytes = Arc::new(res.bytes().await.map_err(|e| e.to_string())?.to_vec());
        *self.cache.lock().unwrap() = Some(Cached { etag, last_modified: last_modified.clone(), bytes: bytes.clone() });
        Ok((bytes, last_modified))
    }

    /// One file from the current release, downloaded (piece-verified) into
    /// `<cache>/client-files/<release>/` once and reused after that.
    pub async fn client_file(&self, path: &str, cache: &Path) -> Result<PathBuf, String> {
        let (bytes, _) = self.fetch_release().await?;
        let t = Torrent::parse(&bytes)?;
        let idx = t.files.iter().position(|f| f.path.eq_ignore_ascii_case(path)).ok_or_else(|| format!("{path} isn't part of the client release"))?;
        let dir = cache.join("client-files").join(&t.info_hash[..12]);
        let out = t.local_path(&dir, idx);
        if std::fs::metadata(&out).map(|m| m.len()).ok() == Some(t.files[idx].length) {
            return Ok(out);
        }
        super::webseed::download(&t, &dir, &[idx], HashSet::new(), Arc::new(AtomicBool::new(false)), &|_| {}, &|_| {}).await?;
        Ok(out)
    }

    fn baseline_paths(&self) -> (PathBuf, PathBuf) {
        (self.dir.join("baseline.json"), self.dir.join("baseline.torrent"))
    }

    /// The last release synced into `wow_dir`, if any.
    pub fn load_baseline(&self, wow_dir: &str) -> Option<(Baseline, Torrent)> {
        let (meta, file) = self.baseline_paths();
        let b: Baseline = read_json(&meta)?;
        if !b.wow_dir.eq_ignore_ascii_case(wow_dir) {
            return None;
        }
        let t = Torrent::parse(&std::fs::read(file).ok()?).ok()?;
        (t.info_hash == b.info_hash).then_some((b, t))
    }

    pub fn save_baseline(&self, wow_dir: &str, torrent: &[u8], info_hash: &str, verified: bool) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let (meta, file) = self.baseline_paths();
        std::fs::write(file, torrent).map_err(|e| e.to_string())?;
        write_json(&meta, &Baseline { wow_dir: wow_dir.into(), info_hash: info_hash.into(), synced_at: now(), verified })
    }
}

// ---------- analysis ----------

pub struct AnalyzeInput<'a> {
    pub release: &'a Torrent,
    pub published: Option<String>,
    pub root: &'a Path,
    pub baseline: Option<(Baseline, Torrent)>,
    pub deep: bool,
}

/// Builds the per-file plan. Blocking (stats files, may hash pieces).
pub fn analyze(
    input: AnalyzeInput,
    log: &(dyn Fn(&'static str, String) + Sync),
    progress: &(dyn Fn(u64, u64) + Sync),
    cancel: &AtomicBool,
) -> Result<Plan, String> {
    let t = input.release;
    let root = input.root;
    let installed = root.join("WoW.exe").is_file();
    let release_changed = input.baseline.as_ref().is_some_and(|(b, _)| b.info_hash != t.info_hash);
    let deltas = input.baseline.as_ref().map(|(_, old)| diff(old, t));

    let mut files: Vec<FileReport> = Vec::with_capacity(t.files.len());
    // Files whose content must be hashed to decide, and the pieces to hash.
    let mut candidates: HashSet<usize> = HashSet::new();
    let mut pieces: Vec<usize> = Vec::new();

    for (i, f) in t.files.iter().enumerate() {
        let local_size = std::fs::metadata(t.local_path(root, i)).ok().filter(|m| m.is_file()).map(|m| m.len());
        let delta = deltas.as_ref().map(|d| &d[i]);
        let is_new = matches!(delta, Some(Delta::New));
        let mut r = FileReport { path: f.path.clone(), size: f.length, local_size, state: FileState::Ok, detail: String::new(), download: false, is_new };

        match (owner(&f.path), local_size) {
            // Plugin DLLs are Client Tweaks' business: ignored by the updater, always.
            (Owner::Tweaks, _) => (r.state, r.detail) = (FileState::Managed, "Managed by Client Tweaks; ignored by the updater".into()),
            (Owner::User, Some(s)) if s != f.length => (r.state, r.detail) = (FileState::UserEdited, "Your edits are kept".into()),
            (_, None) => (r.state, r.download, r.detail) = (FileState::Missing, true, "Not on disk".into()),
            (_, Some(s)) if s != f.length => {
                r.state = FileState::SizeMismatch;
                r.download = true;
                r.detail = match delta {
                    Some(Delta::Resized { old }) if *old == s => format!("Updated in this release ({})", size_change(*old, f.length)),
                    _ if human(s) == human(f.length) => format!("Size on disk is off by {} bytes", s.abs_diff(f.length)),
                    _ => format!("Size on disk is {}, expected {}", human(s), human(f.length)),
                };
                if matches!(delta, Some(Delta::Resized { old }) if *old == s) {
                    r.state = FileState::Outdated;
                }
            }
            (_, Some(_)) => match delta {
                Some(Delta::Changed) => (r.state, r.download, r.detail) = (FileState::Outdated, true, "Changed in this release (same size)".into()),
                Some(Delta::Unknown { pieces: p }) if !input.deep => {
                    candidates.insert(i);
                    pieces.extend(p);
                }
                // Already the new size: probably updated by hand or a previous partial sync.
                Some(Delta::New | Delta::Resized { .. }) if !input.deep => {
                    candidates.insert(i);
                    pieces.extend(t.pieces_for(i));
                }
                _ if input.deep => {
                    candidates.insert(i);
                    pieces.extend(t.pieces_for(i));
                }
                _ => {}
            },
        }
        files.push(r);
    }

    if !pieces.is_empty() {
        pieces.sort_unstable();
        pieces.dedup();
        let bytes: u64 = pieces.iter().map(|&p| t.piece_span(p).end - t.piece_span(p).start).sum();
        log("info", format!("Hash-checking {} file(s) ({})…", candidates.len(), human(bytes)));
        let failed = verify_pieces(t, root, &pieces, progress, cancel);
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let known_bad: HashSet<usize> = (0..t.files.len()).filter(|i| !candidates.contains(i) && files[*i].state != FileState::Ok).collect();
        for i in blame(t, &failed, &known_bad) {
            if !candidates.contains(&i) {
                continue;
            }
            let changed = matches!(deltas.as_ref().map(|d| &d[i]), Some(Delta::Unknown { .. } | Delta::New | Delta::Resized { .. }));
            let r = &mut files[i];
            r.download = true;
            (r.state, r.detail) = if changed {
                (FileState::Outdated, "Changed in this release".into())
            } else {
                (FileState::Corrupt, "Contents don't match the release (corrupted or modified)".into())
            };
        }
    }

    for r in files.iter().filter(|r| !matches!(r.state, FileState::Ok | FileState::Managed)) {
        let level = if r.download { "warn" } else { "info" };
        log(level, format!("{}: {}", r.path, r.detail));
    }

    // Obsolete archives from older clients.
    let in_release: HashSet<String> = t.files.iter().map(|f| f.path.to_ascii_lowercase()).collect();
    let mut obsolete = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("Data")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if !in_release.contains(&format!("data/{lower}")) && LEGACY_ARCHIVES.iter().any(|(n, s)| *n == lower && *s == size) {
                obsolete.push(Obsolete { path: format!("Data/{name}"), size });
            }
        }
    }

    let dropped = input
        .baseline
        .as_ref()
        .map(|(_, old)| old.files.iter().filter(|f| !in_release.contains(&f.path.to_ascii_lowercase())).map(|f| f.path.clone()).collect())
        .unwrap_or_default();

    let download_files = files.iter().filter(|f| f.download).count();
    let download_bytes = files.iter().filter(|f| f.download).map(|f| f.size).sum();
    let status = if !installed && files.iter().filter(|f| f.local_size.is_none()).count() * 2 > files.len() {
        PlanStatus::NotInstalled
    } else if download_files == 0 && obsolete.is_empty() {
        PlanStatus::UpToDate
    } else if release_changed {
        PlanStatus::UpdateAvailable
    } else {
        PlanStatus::RepairNeeded
    };

    Ok(Plan {
        status,
        checked_at: now(),
        release: ReleaseInfo {
            info_hash: t.info_hash.clone(),
            files: t.files.len(),
            total_size: t.total_size,
            created: t.creation_date,
            published: input.published,
            web_seed: t.web_seeds.first().cloned(),
        },
        size_only: input.baseline.is_none() && !input.deep,
        baseline: input.baseline.map(|(b, _)| b),
        release_changed,
        deep_verified: input.deep,
        files,
        download_files,
        download_bytes,
        obsolete,
        dropped,
    })
}

// ---------- download ----------

pub struct DownloadInput<'a> {
    pub updater: &'a Updater,
    pub release: &'a Torrent,
    pub root: &'a Path,
    pub plan: &'a Plan,
}

/// Downloads every file the plan marks for download. Returns how many files were fetched.
pub async fn download(
    input: DownloadInput<'_>,
    log: &(dyn Fn(&'static str, String) + Sync),
    progress: &(dyn Fn(DownloadProgress) + Sync),
) -> Result<usize, String> {
    let DownloadInput { updater, release: t, root, plan } = input;
    let cancel = updater.cancel.clone();
    let wow_dir = root.to_string_lossy().into_owned();
    std::fs::create_dir_all(root).map_err(|e| format!("Couldn't create the game folder: {e}"))?;

    for o in &plan.obsolete {
        log("info", format!("Removing obsolete archive {} ({})", o.path, human(o.size)));
        std::fs::remove_file(root.join(&o.path)).map_err(|e| format!("Couldn't remove {}: {e}", o.path))?;
    }

    let selected: Vec<usize> = plan.files.iter().enumerate().filter(|(_, f)| f.download).map(|(i, _)| i).collect();
    if selected.is_empty() {
        return Ok(0);
    }

    // Resume: pieces a previous attempt at this release + folder already verified
    // and wrote, minus any whose file has since gone missing or changed size.
    std::fs::create_dir_all(&updater.dir).map_err(|e| e.to_string())?;
    let active_path = updater.dir.join("download.json");
    let intact = |fi: usize| std::fs::metadata(t.local_path(root, fi)).map(|m| m.len()).ok() == Some(t.files[fi].length);
    let wanted: HashSet<usize> = selected.iter().flat_map(|&f| t.pieces_for(f)).collect();
    let already: HashSet<usize> = read_json::<ActiveDownload>(&active_path)
        .filter(|p| p.wow_dir == wow_dir && p.info_hash == t.info_hash)
        .map(|p| {
            p.done_pieces
                .into_iter()
                .filter(|piece| wanted.contains(piece) && t.files_in(t.piece_span(*piece)).filter(|f| selected.contains(f)).all(intact))
                .collect()
        })
        .unwrap_or_default();
    if !already.is_empty() {
        log("info", format!("Resuming: {} of {} pieces were already downloaded", already.len(), wanted.len()));
    }

    let total: u64 = selected.iter().map(|&i| t.files[i].length).sum();
    log("info", format!("Downloading {} file(s), {} from the OctoWoW mirror", selected.len(), human(total)));

    let checkpoint = |done: &HashSet<usize>| {
        let _ = write_json(&active_path, &ActiveDownload { wow_dir: wow_dir.clone(), info_hash: t.info_hash.clone(), done_pieces: done.iter().copied().collect() });
    };
    let report = |p: super::webseed::Progress| {
        let files: Vec<FileProgress> = selected.iter().zip(&p.done).map(|(&fi, &done)| FileProgress { path: t.files[fi].path.clone(), size: t.files[fi].length, done }).collect();
        let bytes_done: u64 = files.iter().map(|f| f.done.min(f.size)).sum();
        progress(DownloadProgress {
            phase: if p.speed > 0 || bytes_done > 0 { "downloading" } else { "connecting" },
            bytes_done,
            bytes_total: total,
            speed: p.speed,
            connections: p.active,
            files,
        });
    };
    super::webseed::download(t, root, &selected, already, cancel, &report, &checkpoint).await?;

    let short: Vec<&str> = selected.iter().filter(|&&i| !intact(i)).map(|&i| t.files[i].path.as_str()).collect();
    let _ = std::fs::remove_file(&active_path);
    if !short.is_empty() {
        return Err(format!("Download finished but {} file(s) are incomplete: {}", short.len(), short.join(", ")));
    }
    Ok(selected.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_rules() {
        assert!(owner("nampower.dll") == Owner::Tweaks);
        assert!(owner("d3d9.dll") == Owner::Tweaks);
        assert!(owner("VanillaFixes.exe") == Owner::Tweaks);
        assert!(owner("fmod.dll") == Owner::Client);
        assert!(owner("WoW.exe") == Owner::Client);
        assert!(owner("realmlist.wtf") == Owner::User);
        assert!(owner("Data/patch-3.mpq") == Owner::Client);
    }

    // Live tests against the real OctoWoW release. Run with:
    //   OCTOWOW_TEST_CLIENT=<client dir> cargo test live_ -- --ignored --nocapture

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("octo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    #[ignore]
    async fn live_check_real_client() {
        let root = PathBuf::from(std::env::var("OCTOWOW_TEST_CLIENT").expect("set OCTOWOW_TEST_CLIENT"));
        let up = Updater::new(temp_dir("check"));
        let (bytes, published) = up.fetch_release().await.unwrap();
        let t = Torrent::parse(&bytes).unwrap();
        let start = std::time::Instant::now();
        let plan = analyze(
            AnalyzeInput { release: &t, published, root: &root, baseline: None, deep: false },
            &|l, m| println!("[{l}] {m}"),
            &|_, _| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        println!("checked {} files in {:?}; {} to download ({})", plan.files.len(), start.elapsed(), plan.download_files, human(plan.download_bytes));
        // The ETag cache should make the second fetch a 304.
        let (again, _) = up.fetch_release().await.unwrap();
        assert_eq!(*again, *bytes);
    }

    #[tokio::test]
    #[ignore]
    async fn live_download_small_files() {
        let data = temp_dir("dl-data");
        let root = temp_dir("dl-client");
        let up = Updater::new(data);
        let (bytes, _) = up.fetch_release().await.unwrap();
        let t = Torrent::parse(&bytes).unwrap();
        let mut plan = analyze(
            AnalyzeInput { release: &t, published: None, root: &root, baseline: None, deep: false },
            &|_, _| {},
            &|_, _| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        // Only a few small files; their pieces also cover Tweaks-owned DLLs that must not appear.
        let wanted = |p: &str| p == "DivxDecoder.dll" || p.ends_with(".pub");
        for f in plan.files.iter_mut() {
            f.download = wanted(&f.path);
        }
        plan.obsolete.clear();
        let got = download(
            DownloadInput { updater: &up, release: &t, root: &root, plan: &plan },
            &|l, m| println!("[{l}] {m}"),
            &|p| println!("{} / {} @ {}/s", human(p.bytes_done), human(p.bytes_total), human(p.speed)),
        )
        .await
        .unwrap();
        assert_eq!(got, 13);
        for (i, f) in t.files.iter().enumerate() {
            let exists = t.local_path(&root, i).is_file();
            assert_eq!(exists, wanted(&f.path), "{} present={exists}", f.path);
            if exists {
                assert_eq!(std::fs::metadata(t.local_path(&root, i)).unwrap().len(), f.length);
            }
        }
        assert!(!up.dir.join("download.json").exists(), "resume state cleared after success");
        std::fs::remove_dir_all(&root).ok();
    }

    /// Cancel part-way through a 54 MB file, resume, then hash-check the result.
    #[tokio::test]
    #[ignore]
    async fn live_download_cancel_and_resume() {
        use crate::core::verify::verify_pieces;
        let root = temp_dir("resume-client");
        let up = Updater::new(temp_dir("resume-data"));
        let (bytes, _) = up.fetch_release().await.unwrap();
        let t = Torrent::parse(&bytes).unwrap();
        let mut plan = analyze(AnalyzeInput { release: &t, published: None, root: &root, baseline: None, deep: false }, &|_, _| {}, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        plan.files.iter_mut().for_each(|f| f.download = f.path == "Data/patch-5.mpq");
        plan.obsolete.clear();
        let input = || DownloadInput { updater: &up, release: &t, root: &root, plan: &plan };

        let cancel = up.cancel.clone();
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(2500)).await;
            cancel.store(true, Ordering::SeqCst);
        });
        let first = download(input(), &|l, m| println!("[{l}] {m}"), &|p| println!("{} / {} @ {}/s ({} conns)", human(p.bytes_done), human(p.bytes_total), human(p.speed), p.connections)).await;
        stopper.await.unwrap();
        println!("first attempt: {first:?}");
        assert!(first.unwrap_err().starts_with("Cancelled"));
        let saved: ActiveDownload = read_json(&up.dir.join("download.json")).expect("resume state saved");
        println!("{} pieces saved for resume", saved.done_pieces.len());
        assert!(!saved.done_pieces.is_empty());

        up.cancel.store(false, Ordering::SeqCst);
        let logs = Mutex::new(Vec::new());
        assert_eq!(download(input(), &|_, m| logs.lock().unwrap().push(m), &|_| {}).await.unwrap(), 1);
        assert!(logs.lock().unwrap().iter().any(|m| m.starts_with("Resuming")), "second run resumed");

        // Pieces wholly inside the file must match the release (edges share bytes with absent neighbours).
        let fi = t.files.iter().position(|f| f.path == "Data/patch-5.mpq").unwrap();
        let f = &t.files[fi];
        let interior: Vec<usize> = t.pieces_for(fi).filter(|&p| t.piece_span(p).start >= f.offset && t.piece_span(p).end <= f.offset + f.length).collect();
        assert!(verify_pieces(&t, &root, &interior, &|_, _| {}, &AtomicBool::new(false)).is_empty(), "downloaded data matches the torrent");
        println!("verified {} pieces", interior.len());
        std::fs::remove_dir_all(&root).ok();
    }

    /// The client's own DXVK d3d9.dll, fetched piece-verified and then reused from cache.
    #[tokio::test]
    #[ignore]
    async fn live_client_file() {
        let up = Updater::new(temp_dir("cf-data"));
        let cache = temp_dir("cf-cache");
        let p = up.client_file("d3d9.dll", &cache).await.unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(bytes.len(), 4_501_518);
        assert!(bytes.windows(17).any(|w| w == b"v2.7.1-1-gplasync"), "it's the client's gplasync build");
        let t0 = std::time::Instant::now();
        assert_eq!(up.client_file("d3d9.dll", &cache).await.unwrap(), p);
        println!("cached lookup took {:?}", t0.elapsed());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(2_060_279_959), "1.9 GB");
    }
}
