use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    busy: AtomicBool,
}
impl Updates {
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    version: String,
    current_version: String,
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, Updates>,
) -> Result<Option<Metadata>, String> {
    if state.busy() {
        return Err("An update is already in progress".into());
    }
    let update = app
        .updater_builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let metadata = update.as_ref().map(|u| Metadata {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().unwrap_or_default(),
    });
    *state.pending.lock().map_err(|e| e.to_string())? = update;
    Ok(metadata)
}

struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, Updates>,
    events: Channel<Progress>,
) -> Result<(), String> {
    if state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("An update is already in progress".into());
    }
    let _guard = BusyGuard(&state.busy);
    let mut update = state
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("No pending update. Restart the app to check again.")?;
    update.timeout = Some(Duration::from_secs(15 * 60));
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = events.send(Progress {
                    phase: "downloading",
                    downloaded,
                    total,
                });
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    let _ = events.send(Progress {
        phase: "installing",
        downloaded,
        total: Some(downloaded),
    });
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Stop any live scan/attack/crack before the installer replaces the app.
        crate::stop_sessions(&handle);
        update.install(bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    app.restart();
}
