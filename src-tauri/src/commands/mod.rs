//! IPC surface used by the UI. Anything slow either runs on a blocking
//! thread (scans), as an async command (network), or as a background job
//! (returns a job id immediately and streams progress).

mod addons;
mod client;
mod game;
mod mods;

pub use addons::*;
pub use client::*;
pub use game::*;
pub use mods::*;

use crate::config::Settings;
use crate::core::client as game_dir;
use crate::state::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Tells the UI that cached data for `area` (a query key) is stale.
pub(crate) fn changed(app: &AppHandle, area: &str) {
    let _ = app.emit(crate::jobs::STATE_CHANGED_EVENT, area);
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.snapshot()
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    state.replace(settings)
}

#[tauri::command]
pub async fn validate_game_dir(path: String) -> CmdResult<game_dir::ClientInfo> {
    blocking(move || Ok(game_dir::inspect(&PathBuf::from(path)))).await
}

/// "windows" | "macos" | "linux": the UI hides OS-specific options.
#[tauri::command]
pub fn get_platform() -> &'static str {
    std::env::consts::OS
}
