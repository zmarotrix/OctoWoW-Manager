//! Game Updates tab: check / verify / update the client against the release torrent.

use super::game::run_apply;
use super::CmdResult;
use crate::core::torrent::Torrent;
use crate::core::{platform, updater};
use crate::jobs;
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};


#[derive(Clone, Copy, PartialEq)]
enum ClientOp {
    Check,
    Verify,
    Update,
}

/// Clears the updater's busy flag when the job ends, however it ends.
struct BusyGuard(AppHandle);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.state::<AppState>().updater.busy.store(false, Ordering::SeqCst);
    }
}

fn client_log(app: &AppHandle, level: &'static str, text: String) {
    let _ = app.emit("client-log", updater::LogLine { level, text });
}

fn start_client_op(app: AppHandle, state: &AppState, op: ClientOp) -> CmdResult<u64> {
    let wow_dir = state.wow_dir()?;
    if state.updater.busy.swap(true, Ordering::SeqCst) {
        return Err("A client check or update is already running.".into());
    }
    state.updater.cancel.store(false, Ordering::SeqCst);
    let label = match op {
        ClientOp::Check => "Checking for game updates",
        ClientOp::Verify => "Verifying game files",
        ClientOp::Update => "Updating game files",
    };
    Ok(jobs::spawn(&app.clone(), label, move |ctx| async move {
        let _guard = BusyGuard(app.clone());
        run_client_op(ctx, wow_dir, op).await
    }))
}

/// Fetches the release and builds a plan (hashing as needed) on a worker thread.
async fn plan_for(ctx: &Arc<jobs::JobCtx>, wow_dir: &PathBuf, deep: bool) -> CmdResult<(updater::Plan, Torrent, Arc<Vec<u8>>)> {
    let app = ctx.app().clone();
    let up = &app.state::<AppState>().updater;
    ctx.progress(0, 0, "Fetching release info");
    let (bytes, published) = up.fetch_release().await?;
    let release = Torrent::parse(&bytes)?;
    client_log(&app, "info", format!("Release {} · {} files · {}", &release.info_hash[..8], release.files.len(), updater::human(release.total_size)));

    let baseline = up.load_baseline(&wow_dir.to_string_lossy());
    client_log(&app, "info", match &baseline {
        Some((b, _)) if b.info_hash == release.info_hash => "Same release as your last sync".into(),
        Some(_) => "A new release is out since your last sync".to_string(),
        None => "No previous sync recorded; comparing by file size".into(),
    });

    let (ctx2, root, rel) = (ctx.clone(), wow_dir.clone(), release.clone());
    let cancel = up.cancel.clone();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let app = ctx2.app().clone();
        updater::analyze(
            updater::AnalyzeInput { release: &rel, published, root: &root, baseline, deep },
            &|level, text| client_log(&app, level, text),
            &|done, total| ctx2.progress(done, total, format!("Hash-checking files · {} of {}", updater::human(done), updater::human(total))),
            &cancel,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok((plan, release, bytes))
}

fn publish_plan(ctx: &jobs::JobCtx, plan: updater::Plan) {
    *ctx.app().state::<AppState>().updater.plan.lock().unwrap() = Some(plan);
    ctx.changed("clientUpdate");
}

async fn run_client_op(ctx: Arc<jobs::JobCtx>, wow_dir: PathBuf, op: ClientOp) -> CmdResult<String> {
    let app = ctx.app().clone();
    if op == ClientOp::Update && platform::is_game_running() {
        return Err("Close WoW before updating the client.".into());
    }
    let (plan, release, bytes) = plan_for(&ctx, &wow_dir, op == ClientOp::Verify).await?;
    let up = &app.state::<AppState>().updater;
    let dir_str = wow_dir.to_string_lossy().into_owned();

    // A clean result is a trustworthy baseline for future same-size change detection.
    if plan.status == updater::PlanStatus::UpToDate && (plan.deep_verified || plan.baseline.is_some()) {
        let verified = plan.deep_verified || plan.baseline.as_ref().is_some_and(|b| b.verified && b.info_hash == release.info_hash);
        up.save_baseline(&dir_str, &bytes, &release.info_hash, verified)?;
    }
    let summary = match plan.status {
        updater::PlanStatus::UpToDate => "Your client is up to date".to_string(),
        _ => format!("{} file(s) to download · {}", plan.download_files, updater::human(plan.download_bytes)),
    };
    client_log(&app, if plan.download_files > 0 { "warn" } else { "ok" }, summary.clone());

    if op != ClientOp::Update || plan.status == updater::PlanStatus::UpToDate {
        publish_plan(&ctx, plan);
        return Ok(summary);
    }

    let was_verified = plan.deep_verified;
    let ctx2 = ctx.clone();
    let fetched = updater::download(
        updater::DownloadInput { updater: up, release: &release, root: &wow_dir, plan: &plan },
        &|level, text| client_log(&app, level, text),
        &|p| {
            ctx2.progress(p.bytes_done, p.bytes_total, format!("{} · {}/s", updater::human(p.bytes_done), updater::human(p.speed)));
            let _ = ctx2.app().emit("client-progress", p);
        },
    )
    .await;
    if let Err(e) = fetched {
        client_log(&app, "error", e.clone());
        publish_plan(&ctx, plan);
        return Err(e);
    }

    up.save_baseline(&dir_str, &bytes, &release.info_hash, was_verified)?;
    let count = fetched.unwrap_or(0);
    let mut msg = format!("Updated {count} file(s)");
    client_log(&app, "ok", msg.clone());

    // Like v2.6: new client files mean WoW_Tweaked.exe and plugins need re-applying.
    if count > 0 && wow_dir.join("WoW.exe").is_file() {
        client_log(&app, "info", "Re-applying your tweaks and plugins…".into());
        let settings = app.state::<AppState>().snapshot();
        match run_apply(&app, &settings, &|_, m| ctx.progress(0, 0, m)).await {
            Ok(warnings) => client_log(&app, "ok", format!("Tweaks re-applied{}", if warnings.is_empty() { String::new() } else { format!(" ({} warning(s))", warnings.len()) })),
            Err(e) => {
                client_log(&app, "warn", format!("Couldn't re-apply tweaks: {e}"));
                msg = format!("{msg}; click Apply Changes to re-apply your tweaks");
            }
        }
    }

    let (fresh, _, _) = plan_for(&ctx, &wow_dir, false).await?;
    publish_plan(&ctx, fresh);
    Ok(msg)
}

#[tauri::command]
pub fn client_status(state: State<AppState>) -> Option<updater::Plan> {
    state.updater.plan.lock().unwrap().clone()
}

#[tauri::command]
pub fn client_check(app: AppHandle, state: State<AppState>) -> CmdResult<u64> {
    start_client_op(app, &state, ClientOp::Check)
}

#[tauri::command]
pub fn client_verify(app: AppHandle, state: State<AppState>) -> CmdResult<u64> {
    start_client_op(app, &state, ClientOp::Verify)
}

#[tauri::command]
pub fn client_update(app: AppHandle, state: State<AppState>) -> CmdResult<u64> {
    start_client_op(app, &state, ClientOp::Update)
}

#[tauri::command]
pub fn client_cancel(state: State<AppState>) {
    state.updater.cancel.store(true, Ordering::SeqCst);
}

