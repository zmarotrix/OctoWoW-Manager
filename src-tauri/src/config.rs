//! Persisted user settings. Mirrors the shape of v2.6's `octowow_config.json`
//! so it can be migrated later, but stored camelCase for the UI.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum GpuType {
    /// DXVK 2.6.1 (bundled in the payload).
    #[default]
    Standard,
    /// DXVK 2.5.3 (bundled), an older stable release that avoids AMD-specific crashes.
    #[serde(rename = "AMD")]
    Amd,
    /// DXVK 1.10.3 for GPUs/drivers without Vulkan 1.3.
    Legacy,
    /// No DXVK: the game uses Windows' own DirectX 9.
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PluginSource {
    #[default]
    Recommended,
    Latest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tweaks {
    /// Empty string means "Auto" (detected screen ratio).
    pub aspect_ratio: String,
    pub fov: f64,
    pub farclip: u32,
    pub frill: u32,
    pub nameplate: u32,
    pub max_camera: u32,
    pub sound_channels: u32,
    pub quickloot: bool,
    pub background_sound: bool,
    pub large_address_aware: bool,
    pub camera_skip_fix: bool,
    pub corrupt_interface_bypass: bool,
    pub disable_dep: bool,
}

impl Default for Tweaks {
    fn default() -> Self {
        Self {
            aspect_ratio: String::new(),
            fov: 0.0,
            farclip: 1500,
            frill: 300,
            nameplate: 41,
            max_camera: 100,
            sound_channels: 64,
            quickloot: true,
            background_sound: true,
            large_address_aware: true,
            camera_skip_fix: true,
            corrupt_interface_bypass: true,
            disable_dep: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameModMeta {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub wow_dir: String,
    pub gpu_type: GpuType,
    pub install_autologin: bool,
    pub safety_override: bool,
    pub tweaks: Tweaks,
    /// DLL file name -> enabled. Missing entries fall back to the catalog default.
    pub plugins: BTreeMap<String, bool>,
    pub plugin_sources: BTreeMap<String, PluginSource>,
    pub custom_plugins: BTreeMap<String, bool>,
    /// MPQ base name (without `.disabled`) -> user title/description.
    pub game_mods_meta: BTreeMap<String, GameModMeta>,
    /// Git URLs of addons installed through the manager.
    pub tracked_addons: Vec<String>,
    pub github_token: String,
    /// Linux/macOS: how to run Windows executables (e.g. `wine`, a Proton
    /// wrapper, or a CrossOver/Bottles command). Empty = `wine`.
    pub launch_command: String,
    /// Set when staged settings changed since the last successful Apply.
    pub pending_apply: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wow_dir: String::new(),
            gpu_type: GpuType::Standard,
            install_autologin: true,
            safety_override: false,
            tweaks: Tweaks::default(),
            plugins: BTreeMap::new(),
            plugin_sources: BTreeMap::new(),
            custom_plugins: BTreeMap::new(),
            game_mods_meta: BTreeMap::new(),
            tracked_addons: Vec::new(),
            github_token: String::new(),
            launch_command: String::new(),
            pending_apply: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Writes via a temp file so a crash mid-save can't corrupt settings.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }
}
