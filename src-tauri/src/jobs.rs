//! Background jobs. Every slow operation runs as a job so commands return
//! immediately; progress is streamed to the UI as `job` events.

use serde::Serialize;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const JOB_EVENT: &str = "job";
pub const STATE_CHANGED_EVENT: &str = "state-changed";

/// Progress events closer together than this are dropped (except the last one).
const EMIT_INTERVAL: Duration = Duration::from_millis(50);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Running,
    Finished,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub id: u64,
    pub label: String,
    pub status: JobStatus,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

pub struct JobCtx {
    app: AppHandle,
    id: u64,
    label: String,
    last_emit: Mutex<Instant>,
}

impl JobCtx {
    pub fn progress(&self, done: u64, total: u64, message: impl Into<String>) {
        let mut last = self.last_emit.lock().unwrap();
        if done < total && last.elapsed() < EMIT_INTERVAL {
            return;
        }
        *last = Instant::now();
        self.emit(JobStatus::Running, done, total, message.into());
    }

    pub fn app(&self) -> &AppHandle {
        &self.app
    }

    /// Tell the UI that cached data for `area` (e.g. "addons") is stale.
    pub fn changed(&self, area: &str) {
        let _ = self.app.emit(STATE_CHANGED_EVENT, area);
    }

    fn emit(&self, status: JobStatus, done: u64, total: u64, message: String) {
        let _ = self.app.emit(
            JOB_EVENT,
            JobEvent { id: self.id, label: self.label.clone(), status, done, total, message },
        );
    }
}

/// Starts `work` on the async runtime and returns its id right away.
/// `work` returns a final message on success or an error string.
pub fn spawn<F, Fut>(app: &AppHandle, label: impl Into<String>, work: F) -> u64
where
    F: FnOnce(std::sync::Arc<JobCtx>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<String, String>> + Send + 'static,
{
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let ctx = std::sync::Arc::new(JobCtx {
        app: app.clone(),
        id,
        label: label.into(),
        last_emit: Mutex::new(Instant::now() - EMIT_INTERVAL),
    });
    ctx.emit(JobStatus::Running, 0, 0, String::new());
    tauri::async_runtime::spawn(async move {
        match work(ctx.clone()).await {
            Ok(msg) => ctx.emit(JobStatus::Finished, 1, 1, msg),
            Err(err) => ctx.emit(JobStatus::Failed, 0, 0, err),
        }
    });
    id
}
