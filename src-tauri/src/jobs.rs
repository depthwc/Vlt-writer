//! Background jobs (imports, exports) with progress events and cancel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    cancel: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

#[derive(Serialize, Clone)]
pub struct JobEvent {
    pub id: u64,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
    pub finished: bool,
    pub error: Option<String>,
    pub result: serde_json::Value,
}

pub struct Ctx {
    pub id: u64,
    label: String,
    total: usize,
    pub app: AppHandle,
    cancel: Arc<AtomicBool>,
}

impl Ctx {
    pub fn progress(&self, done: usize, message: impl Into<String>) {
        let _ = self.app.emit("job", JobEvent { id: self.id, label: self.label.clone(), done, total: self.total, message: message.into(), finished: false, error: None, result: serde_json::Value::Null });
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Jobs {
    /// Run `f` on a worker thread; emits progress and a final event. Returns the job id.
    pub fn spawn<F>(&self, app: &AppHandle, label: &str, total: usize, f: F) -> u64
    where
        F: FnOnce(&Ctx) -> Result<serde_json::Value, String> + Send + 'static,
    {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel.lock().insert(id, cancel.clone());
        let ctx = Ctx { id, label: label.to_string(), total, app: app.clone(), cancel };
        tauri::async_runtime::spawn_blocking(move || {
            ctx.progress(0, "");
            let r = f(&ctx);
            let (error, result) = match r {
                Ok(v) => (None, v),
                Err(e) => (Some(e), serde_json::Value::Null),
            };
            let _ = ctx.app.emit("job", JobEvent { id, label: ctx.label.clone(), done: total, total, message: String::new(), finished: true, error, result });
        });
        id
    }

    pub fn cancel(&self, id: u64) {
        if let Some(c) = self.cancel.lock().get(&id) {
            c.store(true, Ordering::Relaxed);
        }
    }
}
