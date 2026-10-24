//! Starting the game: `VanillaFixes.exe WoW_Tweaked.exe` (as v2.6 did) directly
//! on Windows, or through a Wine-compatible runner elsewhere. Every Play starts
//! another client, so several can run side by side (multiboxing).

use std::path::Path;
use std::process::{Command, Stdio};

/// Default runner when none is configured (Linux/macOS).
pub const DEFAULT_RUNNER: &str = "wine";

pub fn launch(wow_dir: &Path, runner: &str) -> Result<(), String> {
    if !wow_dir.join("WoW.exe").is_file() {
        return Err("WoW.exe wasn't found in your game folder.".into());
    }
    let game = if wow_dir.join("WoW_Tweaked.exe").is_file() { "WoW_Tweaked.exe" } else { "WoW.exe" };
    let (program, args): (&str, Vec<&str>) = if wow_dir.join("VanillaFixes.exe").is_file() { ("VanillaFixes.exe", vec![game]) } else { (game, vec![]) };

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new(wow_dir.join(program));
        c.args(&args);
        c
    } else {
        let runner = if runner.trim().is_empty() { DEFAULT_RUNNER } else { runner.trim() };
        let mut parts = runner.split_whitespace();
        let mut c = Command::new(parts.next().unwrap_or(DEFAULT_RUNNER));
        c.args(parts).arg(program).args(&args);
        c
    };
    cmd.current_dir(wow_dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    }
    cmd.spawn().map(|_| ()).map_err(|e| {
        if cfg!(windows) {
            format!("Couldn't start {program}: {e}")
        } else {
            format!("Couldn't start the game with '{runner}': {e}. Set your Wine command in Game Settings.")
        }
    })
}
