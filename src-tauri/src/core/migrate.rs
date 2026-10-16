//! Imports settings from v2.6's `octowow_config.json`.

use crate::config::{GameModMeta, GpuType, PluginSource, Settings};
use serde_json::Value;
use std::path::Path;

pub struct Imported {
    pub settings: Settings,
    /// v2.6's `client_hash`: SHA-1 of the .torrent it last synced.
    pub client_hash: Option<String>,
    pub summary: String,
}

pub fn import_v26(path: &Path, current: &Settings) -> Result<Imported, String> {
    let raw = std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let v: Value = serde_json::from_slice(&raw).map_err(|_| "That file isn't a v2.6 octowow_config.json".to_string())?;
    if !v.is_object() || (v.get("wow_dir").is_none() && v.get("tweaks").is_none()) {
        return Err("That file isn't a v2.6 octowow_config.json".into());
    }
    let mut s = current.clone();
    let str_of = |v: &Value| v.as_str().map(str::to_string);
    let bools = |v: &Value| v.as_object().into_iter().flatten().filter_map(|(k, b)| b.as_bool().map(|b| (k.clone(), b))).collect::<Vec<_>>();

    if let Some(d) = v.get("wow_dir").and_then(str_of).filter(|d| !d.is_empty()) {
        s.wow_dir = d;
    }
    if let Some(g) = v.get("gpu_type").and_then(Value::as_str) {
        s.gpu_type = if g == "AMD" { GpuType::Amd } else { GpuType::Standard };
    }
    if let Some(b) = v.get("install_autologin").and_then(Value::as_bool) {
        s.install_autologin = b;
    }
    for key in ["core_plugins", "optional_plugins"] {
        if let Some(m) = v.get(key) {
            s.plugins.extend(bools(m));
        }
    }
    if let Some(m) = v.get("custom_plugins") {
        s.custom_plugins.extend(bools(m));
    }
    if let Some(m) = v.get("plugin_sources").and_then(Value::as_object) {
        for (k, src) in m {
            s.plugin_sources.insert(k.clone(), if src.as_str().is_some_and(|x| x.starts_with("Latest")) { PluginSource::Latest } else { PluginSource::Recommended });
        }
    }
    if let Some(m) = v.get("game_mods_meta").and_then(Value::as_object) {
        for (k, meta) in m {
            let title = meta.get("title").and_then(str_of).unwrap_or_default();
            let description = meta.get("desc").or_else(|| meta.get("description")).and_then(str_of).unwrap_or_default();
            s.game_mods_meta.insert(k.clone(), GameModMeta { title, description });
        }
    }
    if let Some(list) = v.get("tracked_addons").and_then(Value::as_array) {
        for url in list.iter().filter_map(Value::as_str) {
            if !s.tracked_addons.iter().any(|u| u.eq_ignore_ascii_case(url)) {
                s.tracked_addons.push(url.to_string());
            }
        }
    }
    if let Some(t) = v.get("tweaks") {
        let num = |k: &str| t.get(k).and_then(Value::as_f64);
        let flag = |k: &str| t.get(k).and_then(Value::as_bool);
        let tw = &mut s.tweaks;
        if let Some(x) = num("vt_fov").filter(|x| *x > 0.0) {
            tw.fov = x;
        }
        for (k, field) in [("vt_farclip", &mut tw.farclip), ("vt_frill", &mut tw.frill), ("vt_nameplate", &mut tw.nameplate), ("vt_maxcam", &mut tw.max_camera), ("vt_soundchan", &mut tw.sound_channels)] {
            if let Some(x) = num(k) {
                *field = x.round() as u32;
            }
        }
        for (k, field) in [
            ("vt_quickloot", &mut tw.quickloot),
            ("vt_bg_sound", &mut tw.background_sound),
            ("vt_laa", &mut tw.large_address_aware),
            ("vt_cam_fix", &mut tw.camera_skip_fix),
            ("vt_dep_fix", &mut tw.disable_dep),
            ("vt_corrupt_bypass", &mut tw.corrupt_interface_bypass),
        ] {
            if let Some(b) = flag(k) {
                *field = b;
            }
        }
        // v2.6 stored labels like "16:9 (Widescreen)" or "Auto (3440x1440)".
        if let Some(r) = t.get("ratio_var").and_then(Value::as_str) {
            tw.aspect_ratio = if r.starts_with("Auto") { String::new() } else { r.split_whitespace().next().unwrap_or("").to_string() };
        }
    }
    s.pending_apply = true;
    let summary = format!(
        "Imported game folder, tweaks, {} plugins, {} custom DLLs, {} mod descriptions and {} tracked addons",
        s.plugins.len(),
        s.custom_plugins.len(),
        s.game_mods_meta.len(),
        s.tracked_addons.len()
    );
    Ok(Imported { settings: s, client_hash: v.get("client_hash").and_then(str_of), summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_v26_config() {
        let dir = std::env::temp_dir().join(format!("octo-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("octowow_config.json");
        std::fs::write(
            &path,
            r#"{"wow_dir": "C:\\WoW", "gpu_type": "AMD", "install_autologin": false,
                "core_plugins": {"ClassicAPI.dll": true, "nampower.dll": false},
                "optional_plugins": {"worldmarkers.dll": true},
                "plugin_sources": {"ClassicAPI.dll": "Latest (GitHub)"},
                "tracked_addons": ["https://github.com/a/b"],
                "game_mods_meta": {"patch-Z.mpq": {"title": "HD", "desc": "Nice"}},
                "tweaks": {"vt_fov": 2.1235, "ratio_var": "21:9 (Ultrawide)", "vt_farclip": 1200, "vt_quickloot": false, "vt_dep_fix": false},
                "client_hash": "abc", "custom_plugins": {"x.dll": true}}"#,
        )
        .unwrap();
        let got = import_v26(&path, &Settings::default()).unwrap();
        let s = got.settings;
        assert_eq!(s.wow_dir, "C:\\WoW");
        assert_eq!(s.gpu_type, GpuType::Amd);
        assert!(!s.install_autologin);
        assert_eq!(s.plugins.get("nampower.dll"), Some(&false));
        assert_eq!(s.plugin_sources.get("ClassicAPI.dll"), Some(&PluginSource::Latest));
        assert_eq!(s.game_mods_meta["patch-Z.mpq"].description, "Nice");
        assert_eq!((s.tweaks.farclip, s.tweaks.aspect_ratio.as_str(), s.tweaks.quickloot), (1200, "21:9", false));
        assert_eq!(got.client_hash.as_deref(), Some("abc"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
