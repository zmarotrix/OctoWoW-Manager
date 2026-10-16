//! Apply Changes, Play, app update check and v2.6 settings import.

use super::CmdResult;
use crate::config::Settings;
use crate::core::{app_update, apply, launch, migrate, torrent::Torrent};
use crate::jobs;
use crate::state::AppState;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

/// Runs the Apply pipeline for `settings`. `step(n, label)` reports progress.
pub(crate) async fn run_apply(app: &AppHandle, settings: &Settings, step: &(dyn Fn(u64, &str) + Sync)) -> CmdResult<Vec<String>> {
    let wow_dir = PathBuf::from(settings.wow_dir.trim());
    if settings.wow_dir.trim().is_empty() {
        return Err("Set your WoW folder in Game Settings first.".into());
    }
    let cache = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let bundled = crate::core::payload::unpack(&cache)?;
    apply::apply(
        apply::ApplyInput { wow_dir: &wow_dir, payload: &bundled.payload, cache: &cache, icon: Some(bundled.icon), settings },
        &|_, _| {},
        step,
    )
    .await
}

#[tauri::command]
pub fn apply_changes(app: AppHandle, state: State<AppState>) -> CmdResult<u64> {
    let settings = state.snapshot();
    state.wow_dir()?;
    apply::check_limits(&settings)?;
    Ok(jobs::spawn(&app.clone(), "Applying changes", move |ctx| async move {
        let warnings = run_apply(&app, &settings, &|n, label| ctx.progress(n, 6, label)).await?;
        ctx.changed("plugins");
        ctx.changed("client");
        Ok(if warnings.is_empty() { "Installation and patching complete".to_string() } else { format!("Done with warnings: {}", warnings.join("; ")) })
    }))
}

#[tauri::command]
pub fn launch_game(state: State<AppState>) -> CmdResult<()> {
    let settings = state.snapshot();
    launch::launch(&state.wow_dir()?, &settings.launch_command)
}

#[tauri::command]
pub async fn check_app_update() -> CmdResult<app_update::AppUpdate> {
    app_update::check().await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    settings: Settings,
    summary: String,
}

/// Reads a v2.6 `octowow_config.json`. If its last client sync matches the
/// current release, that release becomes the updater's baseline.
#[tauri::command]
pub async fn import_legacy_config(state: State<'_, AppState>, path: String) -> CmdResult<ImportResult> {
    let imported = migrate::import_v26(&PathBuf::from(&path), &state.snapshot())?;
    let mut summary = imported.summary;
    if let Some(hash) = imported.client_hash.filter(|_| !imported.settings.wow_dir.is_empty()) {
        if let Ok((bytes, _)) = state.updater.fetch_release().await {
            if crate::core::torrent::hex(&Sha1::digest(&bytes[..])) == hash.to_ascii_lowercase() {
                if let Ok(t) = Torrent::parse(&bytes) {
                    state.updater.save_baseline(&imported.settings.wow_dir, &bytes, &t.info_hash, false)?;
                    summary.push_str(", plus your last client sync");
                }
            }
        }
    }
    Ok(ImportResult { settings: imported.settings, summary })
}
