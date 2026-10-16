//! The plugin payload (DLLs, companion addons, AutoLogin GlueXML, dxvk.conf),
//! the shortcut icon and third-party notices are compiled into the executable,
//! like v2.6's single-file build. They're unpacked once per payload version into
//! the app data folder, because the Apply pipeline copies from real files.

use include_dir::{include_dir, Dir};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PAYLOAD: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/payload");
static ICON: &[u8] = include_bytes!("../../icons/icon.ico");
pub static THIRD_PARTY_NOTICES: &str = include_str!("../../../THIRD_PARTY_NOTICES.md");

pub struct Unpacked {
    pub payload: PathBuf,
    pub icon: PathBuf,
}

/// Content hash of the embedded files, so a new build never reuses stale files.
fn fingerprint() -> &'static str {
    static FP: OnceLock<String> = OnceLock::new();
    FP.get_or_init(|| {
        fn walk(d: &Dir, h: &mut Sha1) {
            for f in d.files() {
                h.update(f.path().to_string_lossy().as_bytes());
                h.update(f.contents());
            }
            for sub in d.dirs() {
                walk(sub, h);
            }
        }
        let mut h = Sha1::new();
        walk(&PAYLOAD, &mut h);
        h.update(ICON);
        super::torrent::hex(&h.finalize())[..12].to_string()
    })
}

/// Unpacks into `<app data>/payload-<hash>/` unless that's already done.
pub fn unpack(app_data: &Path) -> Result<Unpacked, String> {
    let dir = app_data.join(format!("payload-{}", fingerprint()));
    let done = dir.join(".complete");
    let out = Unpacked { payload: dir.join("payload"), icon: dir.join("PurpleWowLogo.ico") };
    if done.is_file() {
        return Ok(out);
    }
    let tmp = app_data.join(format!("payload-{}.tmp", fingerprint()));
    let _ = super::util::remove_dir_force(&tmp);
    std::fs::create_dir_all(tmp.join("payload")).map_err(|e| e.to_string())?;
    PAYLOAD.extract(tmp.join("payload")).map_err(|e| format!("Couldn't unpack the plugin payload: {e}"))?;
    std::fs::write(tmp.join("PurpleWowLogo.ico"), ICON).map_err(|e| e.to_string())?;
    std::fs::write(tmp.join("THIRD_PARTY_NOTICES.md"), THIRD_PARTY_NOTICES).map_err(|e| e.to_string())?;
    std::fs::write(tmp.join(".complete"), fingerprint()).map_err(|e| e.to_string())?;
    let _ = super::util::remove_dir_force(&dir);
    std::fs::rename(&tmp, &dir).map_err(|e| format!("Couldn't unpack the plugin payload: {e}"))?;

    // Drop payloads left behind by older builds.
    for e in std::fs::read_dir(app_data).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("payload-") && e.path() != dir {
            let _ = super::util::remove_dir_force(&e.path());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unpacks_once() {
        let data = std::env::temp_dir().join(format!("octo-payload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let u = super::unpack(&data).unwrap();
        assert!(u.payload.join("VanillaFixes.exe").is_file());
        assert!(u.payload.join("WeirdUtils").is_dir());
        assert!(u.icon.is_file());
        let again = super::unpack(&data).unwrap();
        assert_eq!(again.payload, u.payload);
        std::fs::remove_dir_all(&data).ok();
    }
}
