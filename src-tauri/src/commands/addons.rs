//! Addon Manager: scan, install (URL / zip / catalog), update, remove.

use super::{blocking, changed, CmdResult};
use crate::core::addons::{self, url_key, AddonEntry, CatalogEntry};
use crate::jobs;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn addons_dir(state: &AppState) -> CmdResult<PathBuf> {
    Ok(state.wow_dir()?.join("Interface").join("AddOns"))
}

#[tauri::command]
pub async fn scan_addons(state: State<'_, AppState>) -> CmdResult<Vec<AddonEntry>> {
    let dir = state.wow_dir()?;
    let tracked = state.snapshot().tracked_addons;
    let remote = state.addon_remote.lock().unwrap().clone();
    blocking(move || addons::scan(&dir, &tracked, &remote).map_err(|e| format!("Couldn't read the AddOns folder: {e}"))).await
}

/// Asks every managed addon's repository for its latest commit (8 at a time).
#[tauri::command]
pub fn check_addon_updates(app: AppHandle, state: State<AppState>) -> CmdResult<u64> {
    let dir = state.wow_dir()?;
    let tracked = state.snapshot().tracked_addons;
    Ok(jobs::spawn(&app.clone(), "Checking addons for updates", move |ctx| async move {
        let list = blocking(move || addons::scan(&dir, &tracked, &HashMap::new()).map_err(|e| e.to_string())).await?;
        let mut urls: Vec<String> = list.iter().filter_map(|a| a.url.clone()).collect();
        urls.sort_by_key(|u| url_key(u));
        urls.dedup_by_key(|u| url_key(u));
        let total = urls.len() as u64;
        let mut results: HashMap<String, String> = HashMap::new();
        let mut failed = 0;
        for (i, chunk) in urls.chunks(8).enumerate() {
            let handles: Vec<_> = chunk
                .iter()
                .map(|u| {
                    let u = u.clone();
                    tauri::async_runtime::spawn(async move {
                        let r = match addons::parse_repo(&u) {
                            Ok(repo) => addons::ls_remote(&repo).await.map(|(sha, _)| sha),
                            Err(e) => Err(e),
                        };
                        (u, r)
                    })
                })
                .collect();
            for h in handles {
                match h.await {
                    Ok((u, Ok(sha))) => {
                        results.insert(url_key(&u), sha);
                    }
                    _ => failed += 1,
                }
            }
            ctx.progress(((i + 1) * 8).min(urls.len()) as u64, total, "Asking repositories for their latest commit");
        }
        let st = app.state::<AppState>();
        st.addon_remote.lock().unwrap().extend(results);
        ctx.changed("addons");
        let remote = st.addon_remote.lock().unwrap().clone();
        let n = list.iter().filter(|a| a.url.as_ref().and_then(|u| remote.get(&url_key(u))).is_some_and(|r| a.local_sha.as_ref() != Some(r))).count();
        let mut msg = if n == 0 { "All addons are up to date".to_string() } else { format!("{n} update(s) available") };
        if failed > 0 {
            msg.push_str(&format!(" · {failed} repository(ies) unreachable"));
        }
        Ok(msg)
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    folders: Vec<String>,
    url: String,
}

/// Installs or updates every addon in a repository.
#[tauri::command]
pub async fn addon_install(app: AppHandle, state: State<'_, AppState>, url: String) -> CmdResult<Installed> {
    let dir = addons_dir(&state)?;
    let (folders, sha) = addons::install_from_url(&dir, &url).await?;
    state.addon_remote.lock().unwrap().insert(url_key(&url), sha);
    changed(&app, "addons");
    Ok(Installed { folders, url: url.trim().to_string() })
}

#[tauri::command]
pub async fn addon_install_zip(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<Vec<String>> {
    let dir = addons_dir(&state)?;
    let folders = blocking(move || addons::install_from_zip(&dir, &PathBuf::from(path))).await?;
    changed(&app, "addons");
    Ok(folders)
}

#[tauri::command]
pub fn addon_remove(app: AppHandle, state: State<AppState>, folder: String) -> CmdResult<()> {
    addons::remove(&addons_dir(&state)?, &folder)?;
    changed(&app, "addons");
    Ok(())
}

/// The curated OctoWoW addon catalog, cached for the session.
#[tauri::command]
pub async fn addon_catalog(state: State<'_, AppState>, refresh: Option<bool>) -> CmdResult<Vec<CatalogEntry>> {
    if !refresh.unwrap_or(false) {
        if let Some(c) = state.catalog.lock().unwrap().clone() {
            return Ok(c);
        }
    }
    let list = addons::fetch_catalog().await?;
    *state.catalog.lock().unwrap() = Some(list.clone());
    Ok(list)
}
