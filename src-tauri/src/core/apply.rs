//! "Apply Changes": the v2.6 `run_installation` pipeline.
//!
//! clean unselected → copy base files → DXVK → plugins + dlls.txt →
//! WoW_Tweaked.exe (native vanilla-tweaks port + corrupt-interface bypass +
//! DEP opt-out) → shortcut (Windows).
//!
//! No helper executables, scripts or elevation: everything is done in-process,
//! which keeps antivirus heuristics quiet.

use super::plugins;
use super::patcher::{self, Tweaks};
use super::platform::{self, DepPolicy, VulkanStatus};
use super::util::{copy_dir, get_bytes, read_from_targz, remove_dir_force};
use crate::config::{GpuType, PluginSource, Settings};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ApplyInput<'a> {
    pub wow_dir: &'a Path,
    /// Bundled payload (plugin DLLs, companion addons, AutoLogin GlueXML, dxvk.conf).
    pub payload: &'a Path,
    /// App data folder for downloaded DXVK builds.
    pub cache: &'a Path,
    /// App icon for the shortcut.
    pub icon: Option<PathBuf>,
    pub settings: &'a Settings,
    /// The client release's own d3d9.dll (DXVK "Standard"), if it could be fetched.
    pub client_dxvk: Option<PathBuf>,
    pub vulkan: VulkanStatus,
}

pub type Log<'a> = &'a (dyn Fn(&'static str, String) + Sync);

/// Safe maxima; above these needs "Unlock limits" (same as v2.6).
pub fn check_limits(s: &Settings) -> Result<(), String> {
    let t = &s.tweaks;
    if s.safety_override {
        return Ok(());
    }
    if t.farclip > 1500 || t.frill > 300 || t.nameplate > 41 || t.max_camera > 100 || t.sound_channels > 64 || t.fov > 2.2689 {
        return Err("One of your engine values exceeds the safe limit. Lower it, or turn on \"Unlock limits\".".into());
    }
    Ok(())
}

fn plugin_enabled(s: &Settings, p: &plugins::PluginInfo) -> bool {
    s.plugins.get(p.file).copied().unwrap_or(p.default_enabled)
}

/// Finds a child folder by name, ignoring case (Linux file systems are case-sensitive).
fn child_ci(dir: &Path, name: &str) -> PathBuf {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
        .unwrap_or_else(|| dir.join(name))
}

fn addons_dir(wow_dir: &Path) -> PathBuf {
    child_ci(&child_ci(wow_dir, "Interface"), "AddOns")
}

/// A folder the player manages themselves (installed from a repo or a git checkout).
fn user_managed(dir: &Path) -> bool {
    dir.join(".octowow_meta").is_file() || dir.join(".git").exists()
}

pub async fn apply(input: ApplyInput<'_>, log: Log<'_>, step: &(dyn Fn(u64, &str) + Sync)) -> Result<Vec<String>, String> {
    let ApplyInput { wow_dir, payload, cache, settings: s, .. } = input;
    let client_dxvk = input.client_dxvk.clone();
    let mut warnings = Vec::new();
    if !wow_dir.join("WoW.exe").is_file() {
        return Err("WoW.exe wasn't found in your game folder.".into());
    }
    check_limits(s)?;
    if super::platform::is_game_running() {
        return Err("Close WoW before applying changes.".into());
    }
    let catalog = plugins::catalog();
    let addons = addons_dir(wow_dir);

    // 1. Remove what's switched off.
    step(1, "Removing disabled plugins");
    if !s.install_autologin {
        let glue = wow_dir.join("Data/Interface/GlueXML");
        for f in ["AutoLogin.lua", "AutoLogin.xml", "GlueXML.toc"] {
            let _ = std::fs::remove_file(glue.join(f));
        }
    }
    for p in catalog.iter().filter(|p| !plugin_enabled(s, p)) {
        if std::fs::remove_file(wow_dir.join(p.file)).is_ok() {
            log("info", format!("Removed {}", p.file));
        }
        if let Some(companion) = p.companion_addon {
            let dir = child_ci(&addons, companion);
            if dir.is_dir() && !user_managed(&dir) {
                let _ = remove_dir_force(&dir);
                log("info", format!("Removed companion addon {companion}"));
            }
        }
    }

    // 2. Base files.
    step(2, "Copying base files");
    if s.install_autologin && payload.join("Data").is_dir() {
        copy_dir(&payload.join("Data"), &wow_dir.join("Data")).map_err(|e| format!("Couldn't copy AutoLogin files: {e}"))?;
    }
    let disabled_companions: HashSet<String> =
        catalog.iter().filter(|p| !plugin_enabled(s, p)).filter_map(|p| p.companion_addon.map(str::to_ascii_lowercase)).collect();
    let payload_addons = child_ci(&child_ci(payload, "Interface"), "AddOns");
    for e in std::fs::read_dir(&payload_addons).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.path().is_dir() || disabled_companions.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        let dest = child_ci(&addons, &name);
        if dest.is_dir() && user_managed(&dest) {
            log("info", format!("Kept your own copy of {name}"));
            continue;
        }
        copy_dir(&e.path(), &dest).map_err(|e| format!("Couldn't copy addon {name}: {e}"))?;
    }
    for f in ["VanillaFixes.exe", "VfPatcher.dll", "dxvk.conf"] {
        if payload.join(f).is_file() {
            std::fs::copy(payload.join(f), wow_dir.join(f)).map_err(|e| format!("Couldn't copy {f} (is the game running?): {e}"))?;
        }
    }

    // 3. DXVK.
    step(3, "Setting up DXVK");
    let (dxvk, why) = effective_dxvk(s.gpu_type, &input.vulkan);
    if let Some(w) = why {
        log("warn", w.clone());
        warnings.push(w);
    }
    if dxvk == GpuType::Off {
        if park_d3d9(wow_dir)? {
            log("info", "DXVK off: parked d3d9.dll as d3d9.dll.off; the game uses DirectX 9".into());
        }
    } else {
        match dxvk_d3d9(payload, cache, dxvk, client_dxvk.as_deref()).await {
            Ok((bytes, label)) => {
                std::fs::write(wow_dir.join("d3d9.dll"), bytes).map_err(|e| format!("Couldn't write d3d9.dll (is the game running?): {e}"))?;
                let _ = std::fs::remove_file(wow_dir.join("d3d9.dll.off"));
                log("info", format!("Installed {label}"));
            }
            Err(e) => {
                // Bring back a parked copy rather than leaving DXVK half-configured.
                let restored = !wow_dir.join("d3d9.dll").exists() && std::fs::rename(wow_dir.join("d3d9.dll.off"), wow_dir.join("d3d9.dll")).is_ok();
                let w = format!("DXVK not updated: {e}{}", if restored { " (restored your previous d3d9.dll)" } else { "" });
                log("warn", w.clone());
                warnings.push(w);
            }
        }
    }

    // 4. Plugins + dlls.txt.
    step(4, "Installing plugins");
    let mut lines = if dxvk == GpuType::Off { Vec::new() } else { vec!["dxvk".to_string()] };
    for p in catalog.iter().filter(|p| plugin_enabled(s, p)) {
        let dest = wow_dir.join(p.file);
        let latest = p.github.filter(|_| s.plugin_sources.get(p.file) == Some(&PluginSource::Latest));
        let mut installed = false;
        if let Some(repo) = latest {
            match plugins::download_latest(repo, p.file).await {
                Ok((bytes, tag)) => {
                    std::fs::write(&dest, bytes).map_err(|e| format!("Couldn't write {} (is the game running?): {e}", p.file))?;
                    log("info", format!("{} {tag} from GitHub", p.file));
                    installed = true;
                }
                Err(e) => log("warn", format!("{}: latest release unavailable ({e}); using the bundled copy", p.file)),
            }
        }
        if !installed {
            let bundled = [payload.join(p.file), payload.join("WeirdUtils").join(p.file)].into_iter().find(|f| f.is_file());
            if let Some(src) = bundled {
                std::fs::copy(src, &dest).map_err(|e| format!("Couldn't copy {} (is the game running?): {e}", p.file))?;
            }
        }
        if dest.is_file() {
            lines.push(p.file.to_string());
        } else {
            let w = format!("{} isn't bundled and wasn't downloaded; skipped", p.file);
            log("warn", w.clone());
            warnings.push(w);
        }
    }
    for (file, on) in &s.custom_plugins {
        if *on && wow_dir.join(file).is_file() {
            lines.push(file.clone());
        }
    }
    write_dlls_txt(wow_dir, lines, &catalog, s)?;

    // 5. WoW_Tweaked.exe: vanilla-tweaks patches + corrupt-interface bypass + DEP opt-out.
    step(5, "Building WoW_Tweaked.exe");
    let extra: &[(usize, u8)] = if s.tweaks.corrupt_interface_bypass { &CORRUPT_BYPASS } else { &[] };
    patcher::build_tweaked(wow_dir, &Tweaks::from_settings(s), extra, s.tweaks.disable_dep)?;
    log("info", "Built WoW_Tweaked.exe".into());

    // 6. DEP + shortcut.
    step(6, "Finishing up");
    if s.tweaks.disable_dep {
        if let Some(w) = dep_advice(platform::dep_policy()) {
            log("warn", w.clone());
            warnings.push(w);
        }
    }
    if cfg!(windows) {
        if let Err(e) = create_shortcut(wow_dir, input.icon.as_deref()) {
            warnings.push(format!("Couldn't create the shortcut: {e}"));
        }
    }
    Ok(warnings)
}

// ---------- dlls.txt ----------

/// Rewrites dlls.txt: `dxvk`, our enabled plugins, then every line we don't manage
/// (kept in order), de-duplicated case-insensitively.
fn write_dlls_txt(wow_dir: &Path, ours: Vec<String>, catalog: &[plugins::PluginInfo], s: &Settings) -> Result<(), String> {
    let path = wow_dir.join("dlls.txt");
    let mut managed: HashSet<String> = catalog.iter().map(|p| p.file.to_ascii_lowercase()).collect();
    managed.extend(s.custom_plugins.keys().map(|k| k.to_ascii_lowercase()));
    managed.insert("dxvk".into());

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let unmanaged = existing.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|l| !l.is_empty() && !managed.contains(&l.to_ascii_lowercase())).map(str::to_string);

    let mut seen = HashSet::new();
    let out: Vec<String> = ours.into_iter().chain(unmanaged).filter(|l| seen.insert(l.to_ascii_lowercase())).collect();
    std::fs::write(&path, out.join("\n")).map_err(|e| format!("Couldn't write dlls.txt: {e}"))
}

// ---------- DXVK ----------

const DXVK_AMD: &str = "2.5.3";
const DXVK_LEGACY: &str = "1.10.3";

/// Steps the chosen DXVK mode down to one this machine can run. DXVK 2.x needs
/// Vulkan 1.3, 1.10.x needs 1.1; without Vulkan the game must use DirectX 9.
/// Returns the mode to install and, if it changed, why.
pub fn effective_dxvk(chosen: GpuType, vk: &VulkanStatus) -> (GpuType, Option<String>) {
    if !vk.checked || chosen == GpuType::Off {
        return (chosen, None);
    }
    let Some(d) = &vk.device else {
        return (GpuType::Off, Some("No Vulkan-capable graphics driver was found, so DXVK was turned off and the game will use DirectX 9. Update your graphics driver to use DXVK.".into()));
    };
    let v = (d.major, d.minor);
    if v < (1, 1) {
        return (GpuType::Off, Some(format!("{} only supports Vulkan {}.{}, which is too old for DXVK; the game will use DirectX 9.", d.name, d.major, d.minor)));
    }
    if v < (1, 3) && matches!(chosen, GpuType::Standard | GpuType::Amd) {
        return (GpuType::Legacy, Some(format!("{} supports Vulkan {}.{}; DXVK 2.x needs 1.3, so the older-GPU build (DXVK {DXVK_LEGACY}) was installed instead.", d.name, d.major, d.minor)));
    }
    (chosen, None)
}

/// Moves d3d9.dll aside as d3d9.dll.off (the same name the OctoLauncher uses).
/// Returns whether anything was parked.
fn park_d3d9(wow_dir: &Path) -> Result<bool, String> {
    let (live, off) = (wow_dir.join("d3d9.dll"), wow_dir.join("d3d9.dll.off"));
    if !live.is_file() {
        return Ok(false);
    }
    let _ = std::fs::remove_file(&off);
    std::fs::rename(&live, &off).map_err(|e| format!("Couldn't turn DXVK off (is the game running?): {e}"))?;
    Ok(true)
}

/// d3d9.dll for the chosen mode: a bundled payload copy (v2.6 layout) first; for
/// Standard the client release's own build; otherwise the official DXVK release
/// (downloaded once and cached).
async fn dxvk_d3d9(payload: &Path, cache: &Path, mode: GpuType, client_dxvk: Option<&Path>) -> Result<(Vec<u8>, String), String> {
    let (folder, version) = match mode {
        GpuType::Amd => ("DXVK_AMD", DXVK_AMD),
        GpuType::Legacy => ("DXVK_Legacy", DXVK_LEGACY),
        GpuType::Standard => ("DXVK_Standard", ""),
        GpuType::Off => return Err("DXVK is off".into()),
    };
    if let Ok(b) = std::fs::read(payload.join(folder).join("d3d9.dll")) {
        return Ok((b, format!("bundled {folder} d3d9.dll")));
    }
    if mode == GpuType::Standard {
        let path = client_dxvk.ok_or("couldn't download the client's DXVK build")?;
        let b = std::fs::read(path).map_err(|e| e.to_string())?;
        return Ok((b, "the client's own DXVK build".into()));
    }
    let cached = cache.join("dxvk").join(version).join("d3d9.dll");
    if let Ok(b) = std::fs::read(&cached) {
        return Ok((b, format!("DXVK {version}")));
    }
    let url = format!("https://github.com/doitsujin/dxvk/releases/download/v{version}/dxvk-{version}.tar.gz");
    let archive = get_bytes(&url).await?;
    let dll = read_from_targz(&archive, &format!("dxvk-{version}/x32/d3d9.dll"))?;
    if let Some(dir) = cached.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&cached, &dll);
    }
    Ok((dll, format!("DXVK {version}")))
}

// ---------- WoW_Tweaked.exe ----------

/// GlueXML signature-check bypass (v2.6 offsets).
const CORRUPT_BYPASS: [(usize, u8); 6] = [(0x2f113a, 0xeb), (0x2f113b, 0x19), (0x2f1158, 0x03), (0x2f11a7, 0x03), (0x2f11f0, 0xeb), (0x2f11f1, 0xb2)];

/// WoW_Tweaked.exe is built without the NX-compat flag, so under Windows' default
/// policies DEP is already off for it, with no admin prompt or PowerShell
/// (v2.6 ran an elevated Set-ProcessMitigation). Only forced policies need the player.
fn dep_advice(policy: DepPolicy) -> Option<String> {
    match policy {
        DepPolicy::OptOut => Some(
            "Windows applies DEP to every program unless excluded. To exclude the game: System Properties → Advanced → Performance Settings → Data Execution Prevention → Add WoW_Tweaked.exe.".into(),
        ),
        DepPolicy::AlwaysOn => Some("Windows is set to force DEP on for every program, so it can't be disabled for WoW_Tweaked.exe.".into()),
        DepPolicy::OptIn | DepPolicy::AlwaysOff | DepPolicy::NotApplicable => None,
    }
}

/// "Play Modernized WoW.lnk" in the game folder → VanillaFixes.exe WoW_Tweaked.exe,
/// written directly as a .lnk file (no scripting host).
#[cfg(windows)]
fn create_shortcut(wow_dir: &Path, icon: Option<&Path>) -> Result<(), String> {
    let vf = wow_dir.join("VanillaFixes.exe");
    let (target, args) = if vf.is_file() { (vf, Some("WoW_Tweaked.exe".to_string())) } else { (wow_dir.join("WoW_Tweaked.exe"), None) };
    let mut link = mslnk::ShellLink::new(&target).map_err(|e| e.to_string())?;
    link.set_arguments(args);
    link.set_working_dir(Some(wow_dir.to_string_lossy().into_owned()));
    link.set_name(Some("Launch Vanilla WoW with VanillaFixes and Tweaks".into()));
    if let Some(src) = icon.filter(|p| p.is_file()) {
        let dest = wow_dir.join("PurpleWowLogo.ico");
        if std::fs::copy(src, &dest).is_ok() {
            link.set_icon_location(Some(dest.to_string_lossy().into_owned()));
        }
    }
    link.create_lnk(wow_dir.join("Play Modernized WoW.lnk")).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn create_shortcut(_: &Path, _: Option<&Path>) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlls_txt_keeps_unmanaged_and_dedupes() {
        let dir = std::env::temp_dir().join(format!("octo-dlls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dlls.txt"), "\u{feff}dxvk\nnampower.dll\nMyHook.dll\n\nbigcursor.dll\nmyhook.dll\nold-custom.dll\n").unwrap();
        let mut s = Settings::default();
        s.custom_plugins.insert("old-custom.dll".into(), false);
        let ours = vec!["dxvk".into(), "ClassicAPI.dll".into(), "nampower.dll".into()];
        write_dlls_txt(&dir, ours, &plugins::catalog(), &s).unwrap();
        let got = std::fs::read_to_string(dir.join("dlls.txt")).unwrap();
        assert_eq!(got, "dxvk\nClassicAPI.dll\nnampower.dll\nMyHook.dll");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Full Apply against a scratch client built from the live release:
    ///   cargo test live_apply -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_apply_end_to_end() {
        use crate::core::torrent::Torrent;
        use crate::core::updater::{analyze, download, AnalyzeInput, DownloadInput, Updater};
        use std::sync::atomic::AtomicBool;

        let base = std::env::temp_dir().join(format!("octo-e2e-{}", std::process::id()));
        let _ = remove_dir_force(&base);
        let wow = base.join("client");
        let up = Updater::new(base.join("appdata"));

        // 1. A minimal real client: WoW.exe + runtime files from the release.
        let (bytes, _) = up.fetch_release().await.unwrap();
        let t = Torrent::parse(&bytes).unwrap();
        let mut plan = analyze(AnalyzeInput { release: &t, published: None, root: &wow, baseline: None, deep: false }, &|_, _| {}, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        for f in plan.files.iter_mut() {
            f.download = matches!(f.path.as_str(), "WoW.exe" | "fmod.dll" | "realmlist.wtf");
        }
        plan.obsolete.clear();
        download(DownloadInput { updater: &up, release: &t, root: &wow, plan: &plan }, &|_, m| println!("[dl] {m}"), &|_| {}).await.unwrap();
        std::fs::create_dir_all(wow.join("Data")).unwrap();
        std::fs::write(wow.join("MyHook.dll"), b"custom").unwrap();
        std::fs::write(wow.join("dlls.txt"), "SomeoneElses.dll
").unwrap();

        // 2. Apply with v2.6-like defaults, one GitHub "Latest" plugin, one custom DLL.
        let payload = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("payload");
        let mut s = Settings { wow_dir: wow.to_string_lossy().into(), ..Settings::default() };
        s.tweaks.fov = 2.1235;
        s.gpu_type = GpuType::Amd; // a downloadable DXVK build (Standard needs the release's copy)
        s.tweaks.disable_dep = true; // only clears the exe's NX-compat flag
        s.plugin_sources.insert("ClassicAPI.dll".into(), PluginSource::Latest);
        s.custom_plugins.insert("MyHook.dll".into(), true);
        let run = |s: Settings| {
            let (wow, payload, cache) = (wow.clone(), payload.clone(), base.join("appdata"));
            async move {
                apply(ApplyInput { wow_dir: &wow, payload: &payload, cache: &cache, icon: None, settings: &s, client_dxvk: None, vulkan: VulkanStatus::default() }, &|l, m| println!("[{l}] {m}"), &|n, m| println!("step {n}: {m}")).await
            }
        };
        let warnings = run(s.clone()).await.unwrap();
        println!("warnings: {warnings:?}");

        let tweaked = std::fs::read(wow.join("WoW_Tweaked.exe")).unwrap();
        assert_eq!(tweaked.len(), std::fs::metadata(wow.join("WoW.exe")).unwrap().len() as usize);
        assert_eq!(tweaked[0x2f113a], 0xeb, "corrupt-interface bypass applied");
        let dlls = std::fs::read_to_string(wow.join("dlls.txt")).unwrap();
        println!("dlls.txt:
{dlls}");
        assert!(dlls.starts_with("dxvk
ClassicAPI.dll
nampower.dll"));
        assert!(dlls.contains("transmogfix.dll") && dlls.contains("weirdperformance.dll"), "v2.6 skipped these; fixed");
        assert!(dlls.ends_with("MyHook.dll
SomeoneElses.dll"), "custom + unmanaged lines kept");
        assert!(wow.join("d3d9.dll").is_file() && wow.join("dxvk.conf").is_file() && wow.join("VanillaFixes.exe").is_file());
        assert!(wow.join("Data/Interface/GlueXML/AutoLogin.lua").is_file());
        assert!(child_ci(&addons_dir(&wow), "SuperAPI").is_dir());
        if cfg!(windows) {
            assert!(wow.join("Play Modernized WoW.lnk").is_file());
        }

        // 3. Turn a plugin with a companion addon and autologin off; re-apply.
        s.plugins.insert("SuperWoWhook.dll".into(), false);
        s.install_autologin = false;
        run(s.clone()).await.unwrap();
        assert!(!wow.join("SuperWoWhook.dll").exists());
        assert!(!child_ci(&addons_dir(&wow), "SuperAPI").exists(), "companion removed and not copied back");
        assert!(!std::fs::read_to_string(wow.join("dlls.txt")).unwrap().contains("SuperWoWhook"));
        assert!(!wow.join("Data/Interface/GlueXML/AutoLogin.lua").exists());

        remove_dir_force(&base).unwrap();
    }

    #[test]
    fn dxvk_steps_down_for_weak_or_missing_vulkan() {
        use crate::core::platform::VulkanDevice;
        let gpu = |major, minor| VulkanStatus { checked: true, device: Some(VulkanDevice { name: "GPU".into(), vendor_id: 0x10de, major, minor }) };
        assert_eq!(effective_dxvk(GpuType::Standard, &gpu(1, 3)).0, GpuType::Standard);
        assert_eq!(effective_dxvk(GpuType::Amd, &gpu(1, 2)).0, GpuType::Legacy);
        assert_eq!(effective_dxvk(GpuType::Legacy, &gpu(1, 1)), (GpuType::Legacy, None));
        assert_eq!(effective_dxvk(GpuType::Standard, &gpu(1, 0)).0, GpuType::Off);
        assert_eq!(effective_dxvk(GpuType::Standard, &VulkanStatus { checked: true, device: None }).0, GpuType::Off);
        assert_eq!(effective_dxvk(GpuType::Standard, &VulkanStatus::default()), (GpuType::Standard, None), "unchecked (macOS) keeps the choice");
        assert_eq!(effective_dxvk(GpuType::Off, &gpu(1, 3)).0, GpuType::Off);
    }

    #[test]
    fn park_moves_d3d9_aside() {
        let dir = std::env::temp_dir().join(format!("octo-park-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("d3d9.dll"), b"dxvk").unwrap();
        assert!(park_d3d9(&dir).unwrap());
        assert!(!dir.join("d3d9.dll").exists() && dir.join("d3d9.dll.off").is_file());
        assert!(!park_d3d9(&dir).unwrap(), "nothing left to park");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn limits() {
        let mut s = Settings::default();
        assert!(check_limits(&s).is_ok());
        s.tweaks.farclip = 5000;
        assert!(check_limits(&s).is_err());
        s.safety_override = true;
        assert!(check_limits(&s).is_ok());
    }
}
