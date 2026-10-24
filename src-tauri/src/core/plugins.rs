//! Catalog of the DLL plugins shown on the Client Tweaks tab, plus custom DLLs
//! and "Latest (GitHub)" downloads. Installing happens in `apply`.

use super::util::{get_bytes, http};
use serde::Serialize;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginGroup {
    Core,
    Optional,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub file: &'static str,
    pub group: PluginGroup,
    pub description: &'static str,
    pub default_enabled: bool,
    /// `owner/repo` when a "Latest (GitHub)" source is available.
    pub github: Option<&'static str>,
    /// Addon folder that ships alongside the DLL.
    pub companion_addon: Option<&'static str>,
}

const fn core(file: &'static str, description: &'static str) -> PluginInfo {
    PluginInfo { file, group: PluginGroup::Core, description, default_enabled: true, github: None, companion_addon: None }
}

const fn optional(file: &'static str, description: &'static str) -> PluginInfo {
    PluginInfo { file, group: PluginGroup::Optional, description, default_enabled: false, github: None, companion_addon: None }
}

pub fn catalog() -> Vec<PluginInfo> {
    vec![
        PluginInfo {
            github: Some("brues-code/ClassicAPI"),
            ..core("ClassicAPI.dll", "Backports a massive part of the modern WoW API (550+ functions, Lua 5.1 syntax).")
        },
        PluginInfo {
            companion_addon: Some("nampowersettings"),
            ..core("nampower.dll", "Implements ping compensation. Bypasses a 1.12 client flaw to enable queueing spell casts.")
        },
        core("no1600x1200.dll", "Removes the hardcoded 1600x1200 resolution limit."),
        PluginInfo {
            companion_addon: Some("perfboostsettings"),
            ..core("perf_boost.dll", "Provides dynamic render distance controls (culling).")
        },
        PluginInfo {
            github: Some("balakethelock/SuperWoW"),
            companion_addon: Some("SuperAPI"),
            ..core("SuperWoWhook.dll", "Injects a massively expanded Lua API. Increases macro limit.")
        },
        core("transmogfix.dll", "Eliminates FPS drops caused by rapid equipment visual updates."),
        PluginInfo {
            // "Latest" comes from brues-code's maintained fork; "Recommended" is the bundled copy.
            github: Some("brues-code/UnitXP_SP3"),
            companion_addon: Some("UnitXP_SP3_Addon"),
            ..core("UnitXP_SP3.dll", "Engine-level optimizations replacing legacy assembly.")
        },
        PluginInfo {
            github: Some("isfir/VanillaHelpers"),
            ..core("VanillaHelpers.dll", "Expands engine limits. Raises the client memory allocator from 2GB to 4GB.")
        },
        core("weirdperformance.dll", "Engine-level optimizations: SIMD math replacements, etc."),
        optional("bigcursor.dll", "Upscales the hardware cursor for improved visibility on modern resolutions."),
        optional("customassets.dll", "Enables loading loose game asset files from the Data/ directory."),
        optional("logsessions.dll", "Organizes combat and chat logs into clean, per-character, per-day files automatically."),
        optional("minimapicons.dll", "Adds TBC/WotLK-style minimap tracking icons for NPCs and objects."),
        optional("pngscreenshots.dll", "Saves screenshots as compressed PNG files to completely eliminate frame drops."),
        optional("worldmarkers.dll", "Place up to 5 animated colored markers (Cataclysm style) in the world."),
    ]
}

pub fn is_catalog_plugin(file: &str) -> bool {
    catalog().iter().any(|p| p.file.eq_ignore_ascii_case(file))
}

/// Downloads the newest GitHub release's copy of `file`: a `.dll` asset, or the
/// matching file inside a `.zip` asset (v2.6 only handled bare DLLs).
/// Returns `(bytes, tag)`.
pub async fn download_latest(repo: &str, file: &str) -> Result<(Vec<u8>, String), String> {
    let api = format!("https://api.github.com/repos/{repo}/releases/latest");
    let res = http().get(&api).header("Accept", "application/vnd.github+json").send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("GitHub answered HTTP {}", res.status()));
    }
    let release: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().unwrap_or("latest").to_string();
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let named = |pred: &dyn Fn(&str) -> bool| {
        assets.iter().find(|a| a["name"].as_str().is_some_and(pred)).and_then(|a| a["browser_download_url"].as_str()).map(str::to_string)
    };
    let lower = file.to_ascii_lowercase();
    if let Some(url) = named(&|n| n.eq_ignore_ascii_case(file)).or_else(|| named(&|n| n.to_ascii_lowercase().ends_with(".dll"))) {
        return Ok((get_bytes(&url).await?, tag));
    }
    if let Some(url) = named(&|n| n.to_ascii_lowercase().ends_with(".zip")) {
        let bytes = get_bytes(&url).await?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().map_err(|e| e.to_string())?.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
            if name == lower {
                let mut out = Vec::new();
                entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
                return Ok((out, tag));
            }
        }
        return Err(format!("{file} not found in the {tag} release zip"));
    }
    Err(format!("the {tag} release has no {file}"))
}

/// Copies a custom DLL into the game folder. Returns its file name.
pub fn add_custom(wow_dir: &Path, src: &Path) -> Result<String, String> {
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("Not a file")?;
    if !name.to_ascii_lowercase().ends_with(".dll") {
        return Err(format!("{name} is not a .dll"));
    }
    if is_catalog_plugin(&name) {
        return Err(format!("{name} is already managed by the manager; toggle it above instead."));
    }
    if name.eq_ignore_ascii_case("d3d9.dll") {
        return Err("d3d9.dll is managed by the DXVK setting.".into());
    }
    let dest = wow_dir.join(&name);
    let same = std::fs::canonicalize(src).ok().zip(std::fs::canonicalize(&dest).ok()).is_some_and(|(a, b)| a == b);
    if !same {
        std::fs::copy(src, &dest).map_err(|e| format!("Couldn't copy {name} into your game folder: {e}"))?;
    }
    Ok(name)
}

pub fn remove_custom(wow_dir: &Path, file: &str) -> Result<(), String> {
    if file.contains(['/', '\\', ':']) || is_catalog_plugin(file) {
        return Err(format!("Refusing to delete {file}"));
    }
    match std::fs::remove_file(wow_dir.join(file)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't delete {file} (is the game running?): {e}")),
    }
}

#[cfg(test)]
mod tests {
    /// "Latest (GitHub)" for UnitXP comes from brues-code's fork and must be a 32-bit DLL.
    #[tokio::test]
    #[ignore]
    async fn live_unitxp_latest() {
        let (dll, tag) = super::download_latest("brues-code/UnitXP_SP3", "UnitXP_SP3.dll").await.unwrap();
        let pe = u32::from_le_bytes(dll[0x3c..0x40].try_into().unwrap()) as usize;
        assert_eq!(&dll[pe + 4..pe + 6], &0x14cu16.to_le_bytes(), "32-bit, like WoW");
        println!("UnitXP_SP3.dll {tag}: {} bytes", dll.len());
    }
}
