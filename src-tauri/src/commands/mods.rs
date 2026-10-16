//! Game Mods (MPQ patches) and Client Tweaks (plugin catalog, custom DLLs).

use super::{blocking, changed, CmdResult};
use crate::core::{mpq, plugins};
use crate::state::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn get_plugin_catalog() -> Vec<plugins::PluginInfo> {
    plugins::catalog()
}

#[tauri::command]
pub async fn scan_game_mods(state: State<'_, AppState>) -> CmdResult<Vec<mpq::MpqEntry>> {
    let dir = state.wow_dir()?;
    let meta = state.snapshot().game_mods_meta;
    blocking(move || mpq::scan(&dir, &meta).map_err(|e| format!("Couldn't read the Data folder: {e}"))).await
}

#[tauri::command]
pub fn mpq_set_enabled(app: AppHandle, state: State<AppState>, base_name: String, enabled: bool) -> CmdResult<()> {
    mpq::set_enabled(&state.wow_dir()?, &base_name, enabled)?;
    changed(&app, "gameMods");
    Ok(())
}

#[tauri::command]
pub fn mpq_delete(app: AppHandle, state: State<AppState>, file_name: String) -> CmdResult<()> {
    mpq::delete(&state.wow_dir()?, &file_name)?;
    changed(&app, "gameMods");
    Ok(())
}

/// Copies an .mpq into Data/. Returns the installed file name.
#[tauri::command]
pub async fn mpq_install(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let dir = state.wow_dir()?;
    let name = blocking(move || mpq::install(&dir, &PathBuf::from(path))).await?;
    changed(&app, "gameMods");
    Ok(name)
}

/// Copies a DLL into the game folder. Returns its file name.
#[tauri::command]
pub fn custom_dll_add(state: State<AppState>, path: String) -> CmdResult<String> {
    plugins::add_custom(&state.wow_dir()?, &PathBuf::from(path))
}

#[tauri::command]
pub fn custom_dll_remove(state: State<AppState>, file: String) -> CmdResult<()> {
    plugins::remove_custom(&state.wow_dir()?, &file)
}
