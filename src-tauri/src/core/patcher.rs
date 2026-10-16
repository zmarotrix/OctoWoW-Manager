//! Builds `WoW_Tweaked.exe` from `WoW.exe`: a native port of vanilla-tweaks 1.6.0
//! (<https://github.com/brndd/vanilla-tweaks>), producing byte-identical output.
//!
//! vanilla-tweaks is MIT licensed:
//!
//! > Copyright (c) 2022 brndd
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining a copy of this
//! > software and associated documentation files (the "Software"), to deal in the Software
//! > without restriction, including without limitation the rights to use, copy, modify,
//! > merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
//! > permit persons to whom the Software is furnished to do so, subject to the following
//! > conditions: The above copyright notice and this permission notice shall be included in
//! > all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS",
//! > WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.
//!
//! The camera skip fix in vanilla-tweaks was implemented by Bon (Turtle WoW Discord) and
//! submitted by phamd.

use crate::config::Settings;
use std::path::Path;

/// One patch: what to do when building the exe. `None` = leave the game default.
#[derive(Clone, Debug, PartialEq)]
pub struct Tweaks {
    pub fov: Option<f32>,
    pub farclip: Option<f32>,
    pub frill_distance: Option<f32>,
    pub nameplate_distance: Option<f32>,
    pub sound_channels: Option<u32>,
    pub max_camera_distance: Option<f32>,
    pub sound_in_background: bool,
    pub quickloot: bool,
    pub large_address_aware: bool,
    pub camera_skip_fix: bool,
}

/// vanilla-tweaks read numbers from its command line as `f32`; parsing the same
/// text the same way keeps the output byte-identical.
fn f32_of(text: impl ToString) -> f32 {
    text.to_string().parse().unwrap_or_default()
}

impl Tweaks {
    /// Same rules v2.6 used to build vanilla-tweaks' arguments: a value equal to
    /// the game's default means "don't patch".
    pub fn from_settings(s: &Settings) -> Self {
        let t = &s.tweaks;
        Self {
            fov: ((t.fov - 1.5708).abs() >= 0.0001).then(|| f32_of(t.fov)),
            farclip: (t.farclip != 777).then(|| f32_of(t.farclip)),
            frill_distance: (t.frill != 70).then(|| f32_of(t.frill)),
            nameplate_distance: (t.nameplate != 20).then(|| f32_of(t.nameplate)),
            sound_channels: (t.sound_channels != 12).then_some(t.sound_channels),
            max_camera_distance: (t.max_camera != 50).then(|| f32_of(t.max_camera)),
            sound_in_background: t.background_sound,
            quickloot: t.quickloot,
            large_address_aware: t.large_address_aware,
            camera_skip_fix: t.camera_skip_fix,
        }
    }
}

const LAA_CHARACTERISTICS: usize = 0x126;
const FARCLIP: usize = 0x40FED8;
const FOV: usize = 0x4089B4;
const FRILL_DISTANCE: usize = 0x467958;
const SOUND_IN_BACKGROUND: usize = 0x3A4869;
const SOUND_CHANNELS: usize = 0x435D38;
const QUICKLOOT: [usize; 2] = [0x0C1ECF, 0x0C2B25];
const NAMEPLATE_DISTANCE: usize = 0x40C448;
const MAX_CAMERA_DISTANCE: usize = 0x4089A4;

/// Camera rotation skip glitch fix (code caves + jumps).
const CAMERA_SKIP_FIX: [(usize, &[u8]); 5] = [
    (
        0x02ccd0,
        &[
            0x55, 0x8b, 0x05, 0x48, 0x4e, 0x88, 0x00, 0x8b, 0x0d, 0x44, 0x4e, 0x88, 0x00, 0xe9, 0x33, 0x90, 0x32, 0x00, 0x83, 0xc0, 0x32, 0x83, 0xc1,
            0x32, 0x3b, 0x0d, 0xa8, 0xeb, 0xc4, 0x00, 0x7e, 0x03, 0x83, 0xe9, 0x01, 0x3b, 0x05, 0xac, 0xeb, 0xc4, 0x00, 0x7e, 0x03, 0x83, 0xe8, 0x01,
            0x83, 0xe9, 0x32, 0x83, 0xe8, 0x32, 0x89, 0x05, 0x48, 0x4e, 0x88, 0x00, 0x89, 0x0d, 0x44, 0x4e, 0x88, 0x00, 0x5d, 0xeb, 0x0d,
        ],
    ),
    (0x02d326, &[0xe9, 0xb1, 0x8a, 0x32, 0x00]),
    (0x02d334, &[0x8b, 0x35, 0x48, 0x4e, 0x88, 0x00]),
    (0x355d15, &[0x83, 0xf8, 0x32, 0x7d, 0x03, 0x83, 0xc0, 0x01, 0x83, 0xf9, 0x32, 0x7d, 0x03, 0x83, 0xc1, 0x01, 0xe9, 0xb8, 0x6f, 0xcd, 0xff]),
    (
        0x355ddc,
        &[
            0x8d, 0x4d, 0xf0, 0x51, 0xff, 0x35, 0x00, 0x4e, 0x88, 0x00, 0xff, 0x15, 0x50, 0xf6, 0x7f, 0x00, 0x8b, 0x45, 0xf0, 0x8b, 0x15, 0x44, 0x4e,
            0x88, 0x00, 0xe9, 0x35, 0x75, 0xcd, 0xff,
        ],
    ),
];

/// Highest byte any patch touches; smaller files aren't a 1.12.1 client.
const MIN_LEN: usize = FRILL_DISTANCE + 4;

fn put(exe: &mut [u8], at: usize, bytes: &[u8]) {
    exe[at..at + bytes.len()].copy_from_slice(bytes);
}

/// Applies `t` to a 1.12.1 `WoW.exe` image in place.
pub fn patch(exe: &mut [u8], t: &Tweaks) -> Result<(), String> {
    if exe.len() < MIN_LEN || exe.get(..2) != Some(b"MZ") {
        return Err("WoW.exe doesn't look like a 1.12.1 client (too small or not an exe).".into());
    }
    if t.large_address_aware {
        let c = u16::from_le_bytes([exe[LAA_CHARACTERISTICS], exe[LAA_CHARACTERISTICS + 1]]) | 0x20;
        put(exe, LAA_CHARACTERISTICS, &c.to_le_bytes());
    }
    if let Some(v) = t.farclip {
        put(exe, FARCLIP, &v.to_le_bytes());
    }
    if let Some(v) = t.fov {
        put(exe, FOV, &v.to_le_bytes());
    }
    if let Some(v) = t.frill_distance {
        put(exe, FRILL_DISTANCE, &v.to_le_bytes());
    }
    if t.sound_in_background {
        put(exe, SOUND_IN_BACKGROUND, &[0x27]);
    }
    if let Some(n) = t.sound_channels {
        // Stored as a NUL-terminated decimal string in a 4-byte slot.
        let text = format!("{n}\0");
        if !(1..999).contains(&n) || text.len() > 4 {
            return Err(format!("Sound channel count {n} is out of range (1–998)."));
        }
        put(exe, SOUND_CHANNELS, text.as_bytes());
    }
    if t.quickloot {
        for at in QUICKLOOT {
            put(exe, at, &[0x75]);
        }
    }
    if let Some(v) = t.nameplate_distance {
        put(exe, NAMEPLATE_DISTANCE, &v.to_le_bytes());
    }
    if let Some(v) = t.max_camera_distance {
        put(exe, MAX_CAMERA_DISTANCE, &v.to_le_bytes());
    }
    if t.camera_skip_fix {
        for (at, bytes) in CAMERA_SKIP_FIX {
            put(exe, at, bytes);
        }
    }
    Ok(())
}

/// Clears the PE `IMAGE_DLLCHARACTERISTICS_NX_COMPAT` flag so Windows' default
/// (opt-in) DEP policy leaves the process alone.
pub fn opt_out_of_dep(exe: &mut [u8]) -> Result<(), String> {
    let pe = exe.get(0x3c..0x40).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or("Not a PE file")?;
    if exe.get(pe..pe + 4) != Some(&b"PE\0\0"[..]) {
        return Err("Not a PE file".into());
    }
    let at = pe + 24 + 70; // optional header -> DllCharacteristics
    let flags = exe.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or("Truncated PE header")? & !0x0100;
    exe[at..at + 2].copy_from_slice(&flags.to_le_bytes());
    Ok(())
}

/// Reads `WoW.exe`, applies `t`, the `extra` byte patches and optionally the DEP
/// opt-out, and writes `WoW_Tweaked.exe` via a temp file so a failure never
/// leaves half an exe.
pub fn build_tweaked(wow_dir: &Path, t: &Tweaks, extra: &[(usize, u8)], dep_opt_out: bool) -> Result<(), String> {
    let mut exe = std::fs::read(wow_dir.join("WoW.exe")).map_err(|e| format!("Couldn't read WoW.exe: {e}"))?;
    patch(&mut exe, t)?;
    for &(at, byte) in extra {
        *exe.get_mut(at).ok_or("WoW.exe is smaller than expected")? = byte;
    }
    if dep_opt_out {
        opt_out_of_dep(&mut exe)?;
    }
    let out = wow_dir.join("WoW_Tweaked.exe");
    let tmp = wow_dir.join("WoW_Tweaked.exe.tmp");
    std::fs::write(&tmp, &exe).map_err(|e| format!("Couldn't write WoW_Tweaked.exe: {e}"))?;
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Couldn't replace WoW_Tweaked.exe (is the game running?): {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Vec<u8> {
        let mut v = vec![0u8; MIN_LEN + 16];
        v[..2].copy_from_slice(b"MZ");
        v
    }

    #[test]
    fn writes_expected_bytes() {
        let mut s = Settings::default();
        s.tweaks.fov = 2.1235;
        let t = Tweaks::from_settings(&s);
        let mut exe = blank();
        patch(&mut exe, &t).unwrap();
        assert_eq!(&exe[FOV..FOV + 4], &2.1235f32.to_le_bytes());
        assert_eq!(&exe[FARCLIP..FARCLIP + 4], &1500f32.to_le_bytes());
        assert_eq!(&exe[SOUND_CHANNELS..SOUND_CHANNELS + 3], b"64\0");
        assert_eq!(exe[LAA_CHARACTERISTICS] & 0x20, 0x20);
        assert_eq!(&exe[MAX_CAMERA_DISTANCE..MAX_CAMERA_DISTANCE + 4], &100f32.to_le_bytes());
    }

    #[test]
    fn game_defaults_are_left_alone() {
        let mut s = Settings::default();
        s.tweaks.fov = 1.5708;
        (s.tweaks.farclip, s.tweaks.frill, s.tweaks.nameplate, s.tweaks.sound_channels, s.tweaks.max_camera) = (777, 70, 20, 12, 50);
        let t = Tweaks::from_settings(&s);
        assert_eq!((t.fov, t.farclip, t.sound_channels, t.max_camera_distance), (None, None, None, None));
    }

    /// The command line v2.6 passed to vanilla-tweaks for the same settings.
    fn cli_args(s: &Settings) -> Vec<String> {
        let t = &s.tweaks;
        let mut a: Vec<String> = Vec::new();
        let mut opt = |default: bool, flag: &str, value: String, off: &str| {
            if default {
                a.push(off.into());
            } else {
                a.extend([flag.to_string(), value]);
            }
        };
        opt((t.fov - 1.5708).abs() < 0.0001, "--fov", t.fov.to_string(), "--no-fov");
        opt(t.farclip == 777, "--farclip", t.farclip.to_string(), "--no-farclip");
        opt(t.frill == 70, "--frilldistance", t.frill.to_string(), "--no-frilldistance");
        opt(t.nameplate == 20, "--nameplatedistance", t.nameplate.to_string(), "--no-nameplatedistance");
        opt(t.sound_channels == 12, "--soundchannels", t.sound_channels.to_string(), "--no-soundchannels");
        if t.max_camera != 50 {
            a.extend(["--maxcameradistance".to_string(), t.max_camera.to_string()]);
        }
        for (on, flag) in [(t.quickloot, "--no-quickloot"), (t.background_sound, "--no-sound-in-background"), (t.large_address_aware, "--no-largeaddressaware"), (t.camera_skip_fix, "--no-cameraskipfix")] {
            if !on {
                a.push(flag.into());
            }
        }
        a
    }

    /// Byte-for-byte comparison with the original tool on the release's WoW.exe.
    /// Point OCTOWOW_VANILLA_TWEAKS at v1.6.0 (e.g. the v2.6 repo's vanilla-tweaks.exe):
    ///   OCTOWOW_VANILLA_TWEAKS=… cargo test identical_to_vanilla_tweaks -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn identical_to_vanilla_tweaks() {
        use crate::core::torrent::Torrent;
        use crate::core::updater::{analyze, download, AnalyzeInput, DownloadInput, Updater};
        use std::sync::atomic::AtomicBool;

        let tool = std::env::var("OCTOWOW_VANILLA_TWEAKS").expect("set OCTOWOW_VANILLA_TWEAKS");
        let base = std::env::temp_dir().join("octo-patcher-compare");
        let dir = base.join("client");
        if !dir.join("WoW.exe").is_file() {
            let up = Updater::new(base.join("appdata"));
            let (bytes, _) = up.fetch_release().await.unwrap();
            let t = Torrent::parse(&bytes).unwrap();
            let mut plan = analyze(AnalyzeInput { release: &t, published: None, root: &dir, baseline: None, deep: false }, &|_, _| {}, &|_, _| {}, &AtomicBool::new(false)).unwrap();
            plan.files.iter_mut().for_each(|f| f.download = f.path == "WoW.exe");
            plan.obsolete.clear();
            download(DownloadInput { updater: &up, release: &t, root: &dir, plan: &plan }, &|_, _| {}, &|_| {}).await.unwrap();
        }
        let original = std::fs::read(dir.join("WoW.exe")).unwrap();

        let mut cases: Vec<Settings> = Vec::new();
        let mut s = Settings::default();
        s.tweaks.fov = 2.1235;
        cases.push(s.clone()); // the defaults
        for fov in [1.5708, 1.925, 2.2689, 2.5307, 1.9106, 2.0943, 2.7144] {
            let mut c = s.clone();
            c.tweaks.fov = fov;
            cases.push(c);
        }
        for (farclip, frill, nameplate, sound, cam) in [(777, 70, 20, 12, 50), (10000, 1000, 150, 128, 250), (1234, 199, 33, 99, 51), (2500, 450, 60, 1, 120)] {
            let mut c = s.clone();
            (c.tweaks.farclip, c.tweaks.frill, c.tweaks.nameplate, c.tweaks.sound_channels, c.tweaks.max_camera) = (farclip, frill, nameplate, sound, cam);
            cases.push(c);
        }
        for mask in 0..16u8 {
            let mut c = s.clone();
            c.tweaks.quickloot = mask & 1 != 0;
            c.tweaks.background_sound = mask & 2 != 0;
            c.tweaks.large_address_aware = mask & 4 != 0;
            c.tweaks.camera_skip_fix = mask & 8 != 0;
            cases.push(c);
        }

        for (i, c) in cases.iter().enumerate() {
            let out = base.join(format!("vt-{i}.exe"));
            let status = std::process::Command::new(&tool).args(cli_args(c)).arg("-o").arg(&out).arg(dir.join("WoW.exe")).output().unwrap();
            assert!(status.status.success(), "vanilla-tweaks failed on case {i}");
            let expected = std::fs::read(&out).unwrap();
            let mut ours = original.clone();
            patch(&mut ours, &Tweaks::from_settings(c)).unwrap();
            let diff = expected.iter().zip(&ours).position(|(a, b)| a != b);
            assert!(expected.len() == ours.len() && diff.is_none(), "case {i} differs at {diff:?}: {:?}", cli_args(c));
            let _ = std::fs::remove_file(out);
        }
        println!("{} setting combinations: byte-identical to vanilla-tweaks 1.6.0", cases.len());
    }

    #[test]
    fn dep_opt_out_clears_only_nx_compat() {
        let mut exe = blank();
        exe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let at = 0x80 + 24 + 70;
        exe[at..at + 2].copy_from_slice(&0x0140u16.to_le_bytes());
        opt_out_of_dep(&mut exe).unwrap();
        assert_eq!(u16::from_le_bytes([exe[at], exe[at + 1]]), 0x0040);
        assert!(opt_out_of_dep(&mut blank()).is_err());
    }

    #[test]
    fn rejects_wrong_files() {
        assert!(patch(&mut vec![0u8; 100], &Tweaks::from_settings(&Settings::default())).is_err());
        let mut exe = blank();
        let mut t = Tweaks::from_settings(&Settings::default());
        t.sound_channels = Some(1000);
        assert!(patch(&mut exe, &t).is_err());
    }
}
