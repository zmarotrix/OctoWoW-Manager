//! Custom MPQ patches in `Data/`. Disabled patches are renamed to `*.mpq.disabled`.

use crate::config::GameModMeta;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Stock 1.12 archives that are never shown as mods.
const BASE_MPQS: &[&str] = &[
    "backup.mpq", "base.mpq", "dbc.mpq", "fonts.mpq", "interface.mpq", "misc.mpq",
    "model.mpq", "patch.mpq", "patch-1.mpq", "patch-2.mpq", "patch-3.mpq", "patch-4.mpq",
    "patch-5.mpq", "patch-6.mpq", "patch-7.mpq", "patch-8.mpq", "patch-9.mpq", "patch-a.mpq",
    "sound.mpq", "speech.mpq", "terrain.mpq", "texture.mpq", "wmo.mpq",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MpqEntry {
    /// Name without `.disabled`, used as the metadata key.
    pub base_name: String,
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    pub title: String,
    pub description: String,
}

pub fn is_base_archive(name: &str) -> bool {
    BASE_MPQS.contains(&name.to_ascii_lowercase().as_str())
}

/// Only plain file names; never a path or a stock archive.
fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err(format!("Invalid file name '{name}'"));
    }
    if is_base_archive(name.trim_end_matches(".disabled")) {
        return Err(format!("{name} is a stock client archive and can't be managed here"));
    }
    Ok(())
}

/// Enables or disables a patch by renaming between `x.mpq` and `x.mpq.disabled`.
pub fn set_enabled(wow_dir: &Path, base_name: &str, enabled: bool) -> Result<(), String> {
    check_name(base_name)?;
    let data = wow_dir.join("Data");
    let (on, off) = (data.join(base_name), data.join(format!("{base_name}.disabled")));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if !from.exists() {
        return if to.exists() { Ok(()) } else { Err(format!("{base_name} is no longer in your Data folder")) };
    }
    std::fs::rename(&from, &to).map_err(|e| format!("Couldn't toggle {base_name} (is the game running?): {e}"))
}

pub fn delete(wow_dir: &Path, file_name: &str) -> Result<(), String> {
    check_name(file_name)?;
    let path = wow_dir.join("Data").join(file_name);
    std::fs::remove_file(&path).map_err(|e| format!("Couldn't delete {file_name} (is the game running?): {e}"))
}

/// Copies an .mpq into `Data/` (enabled). Replaces an existing patch of the
/// same name, including a disabled copy. Returns the installed file name.
pub fn install(wow_dir: &Path, src: &Path) -> Result<String, String> {
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("Not a file")?;
    if !name.to_ascii_lowercase().ends_with(".mpq") {
        return Err(format!("{name} is not an .mpq file"));
    }
    check_name(&name)?;
    let data = wow_dir.join("Data");
    if !data.is_dir() {
        return Err("Your game folder has no Data folder".into());
    }
    let dest = data.join(&name);
    let same_file = std::fs::canonicalize(src).ok().zip(std::fs::canonicalize(&dest).ok()).is_some_and(|(a, b)| a == b);
    if !same_file {
        std::fs::copy(src, &dest).map_err(|e| format!("Couldn't copy {name}: {e}"))?;
    }
    let _ = std::fs::remove_file(data.join(format!("{name}.disabled")));
    Ok(name)
}

pub fn scan(wow_dir: &Path, meta: &BTreeMap<String, GameModMeta>) -> std::io::Result<Vec<MpqEntry>> {
    let data = wow_dir.join("Data");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&data)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let lower = file_name.to_ascii_lowercase();
        let (base_name, enabled) = match lower.strip_suffix(".disabled") {
            Some(b) if b.ends_with(".mpq") => (file_name[..b.len()].to_string(), false),
            None if lower.ends_with(".mpq") => (file_name.clone(), true),
            _ => continue,
        };
        if BASE_MPQS.contains(&base_name.to_ascii_lowercase().as_str()) {
            continue;
        }
        let m = meta.get(&base_name).cloned().unwrap_or_default();
        out.push(MpqEntry {
            title: if m.title.is_empty() { base_name.clone() } else { m.title },
            description: m.description,
            size: entry.metadata().map(|m| m.len()).unwrap_or(0),
            base_name,
            file_name,
            enabled,
        });
    }
    out.sort_by_key(|e| e.base_name.to_ascii_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_toggle_delete() {
        let root = std::env::temp_dir().join(format!("octo-mpq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::write(root.join("Data/patch.MPQ"), b"base").unwrap();
        let src = root.join("patch-Z.mpq");
        std::fs::write(&src, b"mod").unwrap();

        assert_eq!(install(&root, &src).unwrap(), "patch-Z.mpq");
        let list = scan(&root, &BTreeMap::new()).unwrap();
        assert_eq!(list.len(), 1, "stock archives are hidden");
        assert!(list[0].enabled);

        set_enabled(&root, "patch-Z.mpq", false).unwrap();
        assert!(root.join("Data/patch-Z.mpq.disabled").is_file());
        assert!(!scan(&root, &BTreeMap::new()).unwrap()[0].enabled);
        set_enabled(&root, "patch-Z.mpq", true).unwrap();

        assert!(set_enabled(&root, "patch.MPQ", false).is_err(), "stock archives are protected");
        assert!(delete(&root, "../patch-Z.mpq").is_err());
        delete(&root, "patch-Z.mpq").unwrap();
        assert!(scan(&root, &BTreeMap::new()).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }
}
