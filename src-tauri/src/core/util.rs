//! Shared helpers: HTTP client, safe archive extraction,
//! and folder copy/replace that survives read-only files.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("OctoWoW-Manager/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("http client")
    })
}

pub async fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let res = http().get(url).send().await.map_err(|e| format!("Couldn't reach {}: {e}", host_of(url)))?;
    if !res.status().is_success() {
        return Err(format!("{} answered HTTP {}", host_of(url), res.status()));
    }
    Ok(res.bytes().await.map_err(|e| e.to_string())?.to_vec())
}

fn host_of(url: &str) -> &str {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url)
}

/// Removes a directory tree, clearing read-only flags first (git objects, etc.).
pub fn remove_dir_force(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if std::fs::remove_dir_all(path).is_ok() {
        return Ok(());
    }
    fn clear(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(p, perms);
            if meta.is_dir() {
                for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                    clear(&e.path());
                }
            }
        }
    }
    clear(path);
    std::fs::remove_dir_all(path)
}

/// Recursively copies `src` into `dest`, overwriting files.
pub fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

/// Puts `src` at `dest`, replacing whatever was there. The old copy is only
/// deleted once the new one is in place, so a failure never leaves `dest` empty.
pub fn replace_dir(src: &Path, dest: &Path) -> Result<(), String> {
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let backup = dest.with_file_name(format!(".octowow-old-{name}"));
    let _ = remove_dir_force(&backup);
    let had_old = dest.exists();
    if had_old {
        std::fs::rename(dest, &backup).map_err(|e| format!("Couldn't replace '{name}' (is the game running?): {e}"))?;
    }
    if let Err(e) = std::fs::rename(src, dest).or_else(|_| copy_dir(src, dest)) {
        if had_old {
            let _ = std::fs::rename(&backup, dest);
        }
        return Err(format!("Couldn't install '{name}': {e}"));
    }
    let _ = remove_dir_force(&backup);
    let _ = remove_dir_force(src);
    Ok(())
}

/// Extracts a zip into `dest`, refusing entries that would escape it.
pub fn extract_zip(reader: impl Read + std::io::Seek, dest: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("Not a valid zip file: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reads one file out of a .tar.gz by its path inside the archive.
pub fn read_from_targz(bytes: &[u8], wanted: &str) -> Result<Vec<u8>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        if path.trim_start_matches("./") == wanted {
            let mut out = Vec::new();
            entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err(format!("{wanted} not found in archive"))
}

/// Unique scratch folder next to `near` (same volume, so renames are cheap).
pub fn scratch_dir(near: &Path, tag: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = near.join(format!(".octowow-{tag}-{nanos:x}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_dir_swaps_contents() {
        let root = std::env::temp_dir().join(format!("octo-util-{}", std::process::id()));
        let _ = remove_dir_force(&root);
        std::fs::create_dir_all(root.join("new/sub")).unwrap();
        std::fs::create_dir_all(root.join("dest")).unwrap();
        std::fs::write(root.join("dest/old.txt"), "old").unwrap();
        std::fs::write(root.join("new/sub/a.txt"), "a").unwrap();
        replace_dir(&root.join("new"), &root.join("dest")).unwrap();
        assert!(root.join("dest/sub/a.txt").is_file());
        assert!(!root.join("dest/old.txt").exists());
        assert!(!root.join("new").exists());
        remove_dir_force(&root).unwrap();
    }
}
