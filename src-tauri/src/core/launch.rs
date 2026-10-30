//! Starting the game: `VanillaFixes.exe WoW_Tweaked.exe` (as v2.6 did) directly
//! on Windows, or through a Wine-compatible runner elsewhere. Every Play starts
//! another client, so several can run side by side (multiboxing).
//!
//! The Wine command is read like a shell line: leading `VAR=value` words set
//! environment variables, quotes keep paths with spaces together, and the
//! program may be a Proton or Wine folder instead of an executable.

use std::path::{Path, PathBuf};
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
        let mut target = vec![program.to_string()];
        target.extend(args.iter().map(|a| a.to_string()));
        let plan = plan(runner, &target, &std::env::vars().collect::<Vec<_>>(), &home(), on_path)?;
        let mut c = Command::new(&plan.program);
        c.args(&plan.args);
        for k in &plan.unset {
            c.env_remove(k);
        }
        c.envs(plan.set.iter().map(|(k, v)| (k, v)));
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

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// What to run, with environment changes applied on top of the app's own.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    pub set: Vec<(String, String)>,
    pub unset: Vec<String>,
}

/// Builds the command for `runner` (the Wine command setting) running `target`
/// (`VanillaFixes.exe WoW_Tweaked.exe`). `env` is the app's environment.
pub fn plan(runner: &str, target: &[String], env: &[(String, String)], home: &Path, on_path: impl Fn(&str) -> bool) -> Result<Plan, String> {
    let words = split_words(if runner.trim().is_empty() { DEFAULT_RUNNER } else { runner })?;
    let first_cmd = words.iter().position(|w| env_assignment(w).is_none()).ok_or("The Wine command only sets variables; add the program to run, e.g. wine")?;
    let (mut set, unset) = appimage_cleanup(env);
    for w in &words[..first_cmd] {
        let (k, v) = env_assignment(w).unwrap();
        set.retain(|(x, _)| x != k);
        set.push((k.to_string(), expand_home(v, home)));
    }
    let lookup = {
        let set = set.clone();
        move |k: &str| set.iter().rev().find(|(x, _)| x == k).map(|(_, v)| v.clone()).or_else(|| env.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone()))
    };

    let program = expand_home(&words[first_cmd], home);
    let mut args: Vec<String> = words[first_cmd + 1..].to_vec();
    let dir = Path::new(&program);
    let program = if dir.is_dir() {
        if dir.join("proton").is_file() {
            if on_path("umu-run") {
                // umu-run sets Proton up the way Steam would (the runtime, the prefix).
                set.push(("PROTONPATH".into(), program.clone()));
                if lookup("GAMEID").is_none() {
                    set.push(("GAMEID".into(), "umu-default".into()));
                }
                "umu-run".to_string()
            } else {
                // Plain Proton needs Steam's variables: the prefix lives in <compat data>/pfx.
                let compat = match lookup("STEAM_COMPAT_DATA_PATH") {
                    Some(c) => c,
                    None => match lookup("WINEPREFIX") {
                        Some(p) if p.trim_end_matches('/').ends_with("/pfx") => p.trim_end_matches('/').trim_end_matches("/pfx").to_string(),
                        Some(p) => p,
                        None => home.join(".local/share/octowow-manager/proton").to_string_lossy().into_owned(),
                    },
                };
                let _ = std::fs::create_dir_all(&compat);
                set.push(("STEAM_COMPAT_DATA_PATH".into(), compat));
                if lookup("STEAM_COMPAT_CLIENT_INSTALL_PATH").is_none() {
                    let steam = [".local/share/Steam", ".steam/steam"].iter().map(|p| home.join(p)).find(|p| p.is_dir()).unwrap_or_else(|| home.join(".local/share/octowow-manager"));
                    set.push(("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), steam.to_string_lossy().into_owned()));
                }
                args.insert(0, "run".into());
                dir.join("proton").to_string_lossy().into_owned()
            }
        } else if let Some(wine) = ["bin/wine", "files/bin/wine", "dist/bin/wine"].iter().map(|p| dir.join(p)).find(|p| p.is_file()) {
            wine.to_string_lossy().into_owned()
        } else {
            return Err(format!("'{program}' is a folder, but not a Proton or Wine folder (no 'proton' or 'bin/wine' in it)."));
        }
    } else {
        program
    };
    args.extend(target.iter().cloned());
    Ok(Plan { program, args, set, unset })
}

/// `NAME=value` with a valid variable name.
fn env_assignment(w: &str) -> Option<(&str, &str)> {
    let (k, v) = w.split_once('=')?;
    let valid = !k.is_empty() && !k.starts_with(|c: char| c.is_ascii_digit()) && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some((k, v))
}

fn expand_home(s: &str, home: &Path) -> String {
    let h = home.to_string_lossy();
    let s = match s.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{h}{rest}"),
        _ => s.to_string(),
    };
    s.replace("${HOME}", &h).replace("$HOME", &h)
}

/// Splits like a shell: whitespace separates words; '...' is literal; "..."
/// and bare words allow backslash escapes.
fn split_words(s: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => cur.push(c),
                        None => return Err("The Wine command has an unclosed ' quote.".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => cur.push(c),
                            Some(c) => {
                                cur.push('\\');
                                cur.push(c);
                            }
                            None => return Err("The Wine command has an unclosed \" quote.".into()),
                        },
                        Some(c) => cur.push(c),
                        None => return Err("The Wine command has an unclosed \" quote.".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    cur.push(c);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// Undoes what an AppImage's launcher adds to the environment (its library,
/// Python and GTK paths), which breaks Wine and Proton. Returns
/// `(variables to set, variables to remove)`.
fn appimage_cleanup(env: &[(String, String)]) -> (Vec<(String, String)>, Vec<String>) {
    let Some(appdir) = env.iter().find(|(k, _)| k == "APPDIR").map(|(_, v)| v.trim_end_matches('/').to_string()).filter(|v| !v.is_empty()) else {
        return (Vec::new(), Vec::new());
    };
    let inside = |p: &str| p == appdir || p.starts_with(&format!("{appdir}/"));
    let mut set = Vec::new();
    let mut unset: Vec<String> = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"].map(String::from).to_vec();
    for (k, v) in env {
        if unset.contains(k) || !v.contains(&appdir) {
            continue;
        }
        let kept: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !inside(p)).collect();
        if kept.is_empty() {
            unset.push(k.clone());
        } else {
            set.push((k.clone(), kept.join(":")));
        }
    }
    // Set by AppRun hooks even when they don't point into the AppImage.
    for k in ["LD_PRELOAD", "PYTHONHOME", "PYTHONPATH"] {
        if env.iter().any(|(x, _)| x == k) && !unset.iter().any(|u| u == k) && !set.iter().any(|(x, _)| x == k) {
            unset.push(k.to_string());
        }
    }
    (set, unset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Vec<String> {
        vec!["VanillaFixes.exe".into(), "WoW_Tweaked.exe".into()]
    }
    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(split_words(r#"  WINEPREFIX="/home/a/My Prefix" '/opt/wine 9/bin/wine' x\ y "q\"t" "#).unwrap(), ["WINEPREFIX=/home/a/My Prefix", "/opt/wine 9/bin/wine", "x y", "q\"t"]);
        assert_eq!(split_words("''").unwrap(), [""]);
        assert!(split_words("\"open").is_err());
    }

    #[test]
    fn plain_and_default_wine() {
        let p = plan("", &target(), &[], Path::new("/home/a"), |_| false).unwrap();
        assert_eq!((p.program.as_str(), p.args.clone()), ("wine", target()));
        let p = plan("WINEPREFIX=~/pfx WINEDEBUG=-all '/opt/my wine/bin/wine64' --x", &target(), &[], Path::new("/home/a"), |_| false).unwrap();
        assert_eq!(p.program, "/opt/my wine/bin/wine64");
        assert_eq!(p.args, ["--x", "VanillaFixes.exe", "WoW_Tweaked.exe"]);
        assert_eq!(p.set, env(&[("WINEPREFIX", "/home/a/pfx"), ("WINEDEBUG", "-all")]));
        assert!(plan("FOO=1", &target(), &[], Path::new("/h"), |_| false).is_err());
    }

    #[test]
    fn proton_and_wine_folders() {
        let root = std::env::temp_dir().join(format!("octo-launch-{}", std::process::id()));
        let proton = root.join("GE-Proton11-11");
        let wine = root.join("lutris-wine/bin");
        std::fs::create_dir_all(&proton).unwrap();
        std::fs::create_dir_all(&wine).unwrap();
        std::fs::write(proton.join("proton"), "").unwrap();
        std::fs::write(wine.join("wine"), "").unwrap();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".local/share/Steam")).unwrap();
        let ps = proton.to_string_lossy().into_owned();

        // With umu-run installed it does the Proton setup.
        let p = plan(&format!("WINEPREFIX=/pfx \"{ps}\""), &target(), &[], &home, |n| n == "umu-run").unwrap();
        assert_eq!((p.program.as_str(), p.args.clone()), ("umu-run", target()));
        assert!(p.set.contains(&("PROTONPATH".into(), ps.clone())) && p.set.contains(&("GAMEID".into(), "umu-default".into())));

        // Without it, Proton runs directly with Steam's variables.
        let p = plan(&format!("WINEPREFIX=/games/wow/pfx \"{ps}\""), &target(), &[], &home, |_| false).unwrap();
        assert_eq!(p.program, proton.join("proton").to_string_lossy());
        assert_eq!(p.args, ["run", "VanillaFixes.exe", "WoW_Tweaked.exe"]);
        assert!(p.set.contains(&("STEAM_COMPAT_DATA_PATH".into(), "/games/wow".into())));
        assert!(p.set.contains(&("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), home.join(".local/share/Steam").to_string_lossy().into_owned())));

        let p = plan(&format!("\"{}\"", root.join("lutris-wine").display()), &target(), &[], &home, |_| false).unwrap();
        assert_eq!(Path::new(&p.program).components().collect::<Vec<_>>(), wine.join("wine").components().collect::<Vec<_>>());
        assert!(plan(&format!("\"{}\"", home.display()), &target(), &[], &home, |_| false).unwrap_err().contains("not a Proton or Wine folder"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn clears_the_appimage_environment() {
        let e = env(&[
            ("APPDIR", "/tmp/.mount_Octo"),
            ("APPIMAGE", "/home/a/Octo.AppImage"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_Octo/usr/lib:/tmp/.mount_Octo/usr/lib64"),
            ("PATH", "/tmp/.mount_Octo/usr/bin:/usr/bin:/bin"),
            ("XDG_DATA_DIRS", "/tmp/.mount_Octo/usr/share:/usr/share"),
            ("PYTHONHOME", "/tmp/.mount_Octo/usr"),
            ("HOME", "/home/a"),
        ]);
        let (set, unset) = appimage_cleanup(&e);
        assert_eq!(set, env(&[("PATH", "/usr/bin:/bin"), ("XDG_DATA_DIRS", "/usr/share")]));
        for k in ["APPDIR", "APPIMAGE", "LD_LIBRARY_PATH", "PYTHONHOME"] {
            assert!(unset.iter().any(|u| u == k), "{k}");
        }
        // The user's own settings win over the cleanup.
        let p = plan("LD_LIBRARY_PATH=/opt/lib wine", &target(), &e, Path::new("/home/a"), |_| false).unwrap();
        assert!(p.set.contains(&("LD_LIBRARY_PATH".into(), "/opt/lib".into())));
        // Outside an AppImage nothing changes.
        assert_eq!(appimage_cleanup(&env(&[("PATH", "/usr/bin")])), (vec![], vec![]));
    }
}
