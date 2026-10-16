//! Game folder detection and validation.

use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub valid: bool,
    pub has_wow_exe: bool,
    pub has_tweaked_exe: bool,
    pub has_data_dir: bool,
    pub has_addons_dir: bool,
    pub message: String,
}

pub fn inspect(dir: &Path) -> ClientInfo {
    let has_wow_exe = dir.join("WoW.exe").is_file();
    let has_tweaked_exe = dir.join("WoW_Tweaked.exe").is_file();
    let has_data_dir = dir.join("Data").is_dir();
    let has_addons_dir = dir.join("Interface").join("AddOns").is_dir();
    let valid = has_wow_exe && has_data_dir;
    let message = if !dir.is_dir() {
        "Folder does not exist.".to_string()
    } else if !has_wow_exe {
        "WoW.exe was not found in this folder.".to_string()
    } else if !has_data_dir {
        "The Data folder is missing.".to_string()
    } else {
        "Valid 1.12 client.".to_string()
    };
    ClientInfo { valid, has_wow_exe, has_tweaked_exe, has_data_dir, has_addons_dir, message }
}
