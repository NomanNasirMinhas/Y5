#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod attack;
mod capture;
mod crack;
mod hashcat;
mod model;
mod preflight;
mod process;
mod replay;
mod updates;
mod validation;
mod wifi;

use model::*;
use process::ChildGuard;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};
use tauri::{ipc::Channel, Emitter, Manager, State};

struct Job {
    guard: Arc<ChildGuard>,
    thread: JoinHandle<()>,
}

// Reap completed workers without signalling a PID that may have been reused.
fn clear_finished(slot: &mut Option<Job>) {
    if slot.as_ref().is_some_and(|job| job.thread.is_finished()) {
        if let Some(job) = slot.take() {
            let _ = job.thread.join();
        }
    }
}

#[tauri::command]
async fn job_running(state: State<'_, AppState>, kind: String) -> Result<bool, String> {
    let slot = match kind.as_str() {
        "scan" => &state.scan,
        "attack" => &state.attack,
        "crack" => &state.crack,
        _ => return Err("Unknown job kind".into()),
    };
    let mut slot = match slot.try_lock() {
        Ok(slot) => slot,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(true),
        Err(e) => return Err(e.to_string()),
    };
    clear_finished(&mut slot);
    Ok(slot.is_some())
}

struct AppState {
    restore_on_exit: std::sync::atomic::AtomicBool,
    closing: std::sync::atomic::AtomicBool,
    radios: Mutex<std::collections::HashSet<String>>,
    scan: Mutex<Option<Job>>,
    attack: Mutex<Option<Job>>,
    crack: Mutex<Option<Job>>,
    scans_root: PathBuf,
    wordlists_root: PathBuf,
    rules_root: PathBuf,
}

#[tauri::command]
fn platform() -> String {
    match wifi::platform() {
        wifi::Platform::Linux => "linux".into(),
        wifi::Platform::Windows => "windows".into(),
        wifi::Platform::Macos => "macos".into(),
    }
}

#[tauri::command]
async fn list_interfaces() -> Result<Vec<WirelessInterface>, String> {
    tauri::async_runtime::spawn_blocking(wifi::list_interfaces)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_monitor(state: State<'_, AppState>, interface: String) -> Result<String, String> {
    validation::interface(&interface)?;

    let name = tauri::async_runtime::spawn_blocking(move || wifi::start_monitor(&interface))
        .await
        .map_err(|e| e.to_string())??;
    state
        .radios
        .lock()
        .map_err(|e| e.to_string())?
        .insert(name.clone());
    Ok(name)
}

#[tauri::command]
async fn stop_monitor(state: State<'_, AppState>, interface: String) -> Result<String, String> {
    validation::interface(&interface)?;

    let name = interface.clone();
    let result = tauri::async_runtime::spawn_blocking(move || wifi::stop_monitor(&interface))
        .await
        .map_err(|e| e.to_string())??;
    state
        .radios
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&name);
    Ok(result)
}

#[tauri::command]
async fn check_kill() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(wifi::check_kill)
        .await
        .map_err(|e| e.to_string())?
}

fn launch_radio(
    app: tauri::AppHandle,
    kind: &str,
    interface: String,
    logs: Channel<LogEvent>,
    launch: impl FnOnce(String) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> + Send + 'static,
) -> Result<(), String> {
    validation::interface(&interface)?;
    let state = app.state::<AppState>();
    let mut scan = state.scan.lock().map_err(|e| e.to_string())?;
    let mut attack = state.attack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut scan);
    clear_finished(&mut attack);
    if scan.is_some() || attack.is_some() || state.closing.load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(
            "A radio operation is still running or restoring its interface. Wait for it to finish."
                .into(),
        );
    }
    let guard = ChildGuard::new();
    let worker_guard = guard.clone();
    let handle = app.clone();
    let thread = std::thread::spawn(move || {
        let report = |level: &str, message: String| {
            let _ = logs.send(LogEvent {
                level: level.into(),
                message,
                timestamp: now(),
            });
        };
        report("info", format!("Preparing {interface} for monitor mode…"));
        let state = handle.state::<AppState>();
        // Track before setup so a partial setup failure remains recoverable on close.
        if let Ok(mut radios) = state.radios.lock() {
            radios.insert(interface.clone());
        }
        let result = (|| {
            if worker_guard.stop_requested() {
                return Ok(());
            }
            let monitor = wifi::start_monitor(&interface)?;
            if worker_guard.stop_requested() {
                return Ok(());
            }
            report("info", format!("{monitor}: monitor mode ready"));
            let (child_guard, child_thread) = launch(monitor)?;
            while !child_thread.is_finished() {
                if worker_guard.stop_requested() {
                    child_guard.request_stop();
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            child_thread
                .join()
                .map_err(|_| "Radio worker failed".to_string())?;
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            report("error", error);
        }
        report("info", format!("Restoring {interface} to managed mode…"));
        match wifi::stop_monitor(&interface) {
            Ok(_) => {
                if let Ok(mut radios) = state.radios.lock() {
                    radios.remove(&interface);
                }
                report("info", format!("{interface}: managed mode restored"));
            }
            Err(error) => {
                report("error", format!("Radio recovery failed: {error}"));
                let _ = handle.emit("recovery-error", error);
            }
        }
    });
    let job = Some(Job { guard, thread });
    if kind == "scan" {
        *scan = job;
    } else {
        *attack = job;
    }
    Ok(())
}

#[tauri::command]
async fn start_scan(
    app: tauri::AppHandle,
    interface: String,
    channel: Option<i32>,
    snapshots: Channel<ScanSnapshot>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    if let Some(ch) = channel {
        validation::channel(ch)?;
    }
    let prefix = app
        .state::<AppState>()
        .scans_root
        .join(format!("scan-{}", now()))
        .to_string_lossy()
        .into_owned();
    let worker_logs = logs.clone();
    launch_radio(app, "scan", interface, logs, move |iface| {
        capture::start(iface, channel, prefix, snapshots, worker_logs)
    })
}

async fn stop_radio(app: tauri::AppHandle, kind: &'static str) -> Result<(), String> {
    // Stop requests return immediately. The supervisor owns shutdown and recovery;
    // job_running remains true until both have finished.
    let state = app.state::<AppState>();
    let slot = if kind == "scan" {
        &state.scan
    } else {
        &state.attack
    };
    let slot = slot.lock().map_err(|e| e.to_string())?;
    if let Some(job) = slot.as_ref() {
        job.guard.request_stop();
    }
    Ok(())
}
#[tauri::command]
async fn stop_scan(app: tauri::AppHandle) -> Result<(), String> {
    stop_radio(app, "scan").await
}
#[tauri::command]
async fn stop_attack(app: tauri::AppHandle) -> Result<(), String> {
    stop_radio(app, "attack").await
}

#[tauri::command]
async fn deauth(
    app: tauri::AppHandle,
    interface: String,
    bssid: String,
    client: Option<String>,
    channel: i32,
    count: u32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::mac(&bssid)?;
    validation::channel(channel)?;
    if let Some(ref c) = client {
        validation::mac(c)?;
    }
    if !(1..=9999).contains(&count) {
        return Err("Packet count must be from 1 to 9999.".into());
    }
    let worker_logs = logs.clone();
    launch_radio(app, "attack", interface, logs, move |iface| {
        wifi::set_channel(&iface, channel)?;
        attack::deauth_job(iface, bssid, client, count, worker_logs)
    })
}
#[tauri::command]
async fn start_deauth(
    app: tauri::AppHandle,
    interface: String,
    bssid: String,
    client: Option<String>,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::mac(&bssid)?;
    validation::channel(channel)?;
    if let Some(ref c) = client {
        validation::mac(c)?;
    }
    let worker_logs = logs.clone();
    launch_radio(app, "attack", interface, logs, move |iface| {
        wifi::set_channel(&iface, channel)?;
        attack::deauth_job(iface, bssid, client, 0, worker_logs)
    })
}
#[tauri::command]
async fn capture_handshake(
    app: tauri::AppHandle,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::mac(&bssid)?;
    validation::channel(channel)?;
    let prefix = app
        .state::<AppState>()
        .scans_root
        .join(format!("hs-{}", now()))
        .to_string_lossy()
        .into_owned();
    let worker_logs = logs.clone();
    launch_radio(app, "attack", interface, logs, move |iface| {
        attack::capture_handshake(iface, bssid, channel, prefix, worker_logs)
    })
}
#[tauri::command]
async fn capture_pmkid(
    app: tauri::AppHandle,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::mac(&bssid)?;
    validation::channel(channel)?;
    let prefix = app
        .state::<AppState>()
        .scans_root
        .join(format!("pmkid-{}", now()))
        .to_string_lossy()
        .into_owned();
    let worker_logs = logs.clone();
    launch_radio(app, "attack", interface, logs, move |iface| {
        attack::capture_pmkid(iface, bssid, channel, prefix, worker_logs)
    })
}
#[tauri::command]
async fn wps_attack(
    app: tauri::AppHandle,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::mac(&bssid)?;
    validation::channel(channel)?;
    let worker_logs = logs.clone();
    launch_radio(app, "attack", interface, logs, move |iface| {
        attack::wps_attack(iface, bssid, channel, worker_logs)
    })
}

#[tauri::command]
async fn start_crack(
    state: State<'_, AppState>,
    capture: String,
    wordlist: String,
    bssid: Option<String>,
    mode: String,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::file(&capture)?;
    validation::file(&wordlist)?;
    if let Some(ref b) = bssid {
        validation::mac(b)?;
    }
    if !["auto", "wpa", "wep"].contains(&mode.as_str()) {
        return Err("Invalid recovery mode.".into());
    }

    let mut slot = state.crack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("A crack job is already running.".into());
    }
    let (guard, thread) = crack::crack_start(capture, wordlist, bssid, mode, status, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn stop_crack(state: State<'_, AppState>) -> Result<(), String> {
    let job = state.crack.lock().map_err(|e| e.to_string())?.take();
    if let Some(job) = job {
        tauri::async_runtime::spawn_blocking(move || stop_job(job))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn start_hashcat(
    state: State<'_, AppState>,
    opts: hashcat::HashcatOptions,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    let mut slot = state.crack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("A crack job is already running.".into());
    }
    let workdir = state.scans_root.join("hashcat");
    let (guard, thread) = hashcat::hashcat_start(opts, workdir, status, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn hashcat_info() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(hashcat::info)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_rules(state: State<'_, AppState>) -> Vec<Wordlist> {
    hashcat::list_rules(std::slice::from_ref(&state.rules_root))
}

#[tauri::command]
async fn export_hashcat(capture: String) -> Result<String, String> {
    validation::file(&capture)?;

    tauri::async_runtime::spawn_blocking(move || crack::export_hashcat(&capture))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_wordlists(state: State<'_, AppState>) -> Vec<Wordlist> {
    crack::list_wordlists(std::slice::from_ref(&state.wordlists_root))
}

#[tauri::command]
fn list_captures(state: State<'_, AppState>) -> Result<Vec<CaptureInfo>, String> {
    replay::list(&state.scans_root)
}

#[tauri::command]
fn reveal_captures(state: State<'_, AppState>) -> Result<(), String> {
    std::fs::create_dir_all(&state.scans_root).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&state.scans_root)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn stop_job(job: Job) {
    if !job.thread.is_finished() {
        job.guard.request_stop();
    }
    let _ = job.thread.join();
}

/// Request a stop on any live scan/attack/crack and join their workers.
pub fn stop_sessions(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    for slot in [&state.scan, &state.attack, &state.crack] {
        if let Ok(mut s) = slot.lock() {
            if let Some(job) = s.take() {
                stop_job(job);
            }
        }
    }
}

#[tauri::command]
async fn recover_interfaces(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        stop_sessions(&app);
        restore_radios(&app)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn restore_radios(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let names: Vec<_> = state
        .radios
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .cloned()
        .collect();
    let mut errors = Vec::new();
    for name in names {
        match wifi::stop_monitor(&name) {
            Ok(_) => {
                state
                    .radios
                    .lock()
                    .map_err(|e| e.to_string())?
                    .remove(&name);
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    if errors.is_empty() {
        Ok("Session radios restored to managed mode.".into())
    } else {
        Err(errors.join("\n"))
    }
}

#[tauri::command]
fn set_restore_on_exit(state: State<'_, AppState>, enabled: bool) {
    state
        .restore_on_exit
        .store(enabled, std::sync::atomic::Ordering::SeqCst);
}

fn shutdown(app: &tauri::AppHandle) {
    if app
        .state::<AppState>()
        .closing
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        stop_sessions(&handle);
        let recover = handle
            .state::<AppState>()
            .restore_on_exit
            .load(std::sync::atomic::Ordering::SeqCst);
        let result = if recover {
            restore_radios(&handle)
        } else {
            handle.state::<AppState>().radios.lock().unwrap().clear();
            Ok(String::new())
        };
        match result {
            Ok(_) => handle.exit(0),
            Err(error) => {
                handle
                    .state::<AppState>()
                    .closing
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = handle.emit("recovery-error", error);
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let scans = data.join("scans");
            let wordlists = data.join("wordlists");
            let rules = data.join("rules");
            std::fs::create_dir_all(&scans).ok();
            std::fs::create_dir_all(&wordlists).ok();
            std::fs::create_dir_all(&rules).ok();
            app.manage(AppState {
                restore_on_exit: std::sync::atomic::AtomicBool::new(true),
                closing: std::sync::atomic::AtomicBool::new(false),
                radios: Mutex::new(std::collections::HashSet::new()),
                scan: Mutex::new(None),
                attack: Mutex::new(None),
                crack: Mutex::new(None),
                scans_root: scans,
                wordlists_root: wordlists,
                rules_root: rules,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform,
            preflight::preflight,
            set_restore_on_exit,
            recover_interfaces,
            job_running,
            list_interfaces,
            start_monitor,
            stop_monitor,
            check_kill,
            start_scan,
            stop_scan,
            deauth,
            start_deauth,
            capture_handshake,
            capture_pmkid,
            wps_attack,
            stop_attack,
            start_crack,
            stop_crack,
            start_hashcat,
            hashcat_info,
            list_rules,
            export_hashcat,
            list_wordlists,
            list_captures,
            reveal_captures,
            updates::check_update,
            updates::install_update
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.state::<updates::Updates>().busy() {
                    return;
                }
                shutdown(window.app_handle());
            }
        })
        .build(tauri::generate_context!())
        .expect("could not build Y5")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                let busy = state.scan.lock().map(|s| s.is_some()).unwrap_or(false)
                    || state.attack.lock().map(|s| s.is_some()).unwrap_or(false)
                    || state.crack.lock().map(|s| s.is_some()).unwrap_or(false)
                    || state.radios.lock().map(|s| !s.is_empty()).unwrap_or(false);
                if busy {
                    api.prevent_exit();
                    shutdown(app);
                }
            }
        });
}

#[cfg(test)]
mod job_tests {
    use super::*;
    #[test]
    fn completed_workers_release_the_slot_without_requesting_stop() {
        let guard = ChildGuard::new();
        let thread = std::thread::spawn(|| {});
        while !thread.is_finished() {
            std::thread::yield_now();
        }
        let mut slot = Some(Job {
            guard: guard.clone(),
            thread,
        });
        clear_finished(&mut slot);
        assert!(slot.is_none());
        assert!(!guard.stop_requested());
    }
}
