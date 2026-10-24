#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod attack;
mod capture;
mod crack;
mod hashcat;
mod model;
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
use tauri::{ipc::Channel, Manager, State};

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
fn job_running(state: State<'_, AppState>, kind: String) -> Result<bool, String> {
    let slot = match kind.as_str() {
        "scan" => &state.scan,
        "attack" => &state.attack,
        "crack" => &state.crack,
        _ => return Err("Unknown job kind".into()),
    };
    let mut slot = slot.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    Ok(slot.is_some())
}

struct AppState {
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
async fn start_monitor(interface: String) -> Result<String, String> {
    validation::interface(&interface)?;

    tauri::async_runtime::spawn_blocking(move || wifi::start_monitor(&interface))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_monitor(interface: String) -> Result<String, String> {
    validation::interface(&interface)?;

    tauri::async_runtime::spawn_blocking(move || wifi::stop_monitor(&interface))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_kill() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(wifi::check_kill)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_scan(
    state: State<'_, AppState>,
    interface: String,
    channel: Option<i32>,
    snapshots: Channel<ScanSnapshot>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    if let Some(ch) = channel {
        validation::channel(ch)?;
    }

    let mut slot = state.scan.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("A scan is already running.".into());
    }
    let prefix = state
        .scans_root
        .join(format!("scan-{}", now()))
        .to_string_lossy()
        .into_owned();
    let (guard, thread) = capture::start(interface, channel, prefix, snapshots, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn stop_scan(state: State<'_, AppState>) -> Result<(), String> {
    let job = state.scan.lock().map_err(|e| e.to_string())?.take();
    if let Some(job) = job {
        tauri::async_runtime::spawn_blocking(move || stop_job(job))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn deauth(
    interface: String,
    bssid: String,
    client: Option<String>,
    count: u32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    validation::mac(&bssid)?;
    if let Some(ref c) = client {
        validation::mac(c)?;
    }
    if !(1..=9999).contains(&count) {
        return Err("Packet count must be from 1 to 9999.".into());
    }

    tauri::async_runtime::spawn_blocking(move || {
        attack::deauth_once(&interface, &bssid, client.as_deref(), count, &logs)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_deauth(
    state: State<'_, AppState>,
    interface: String,
    bssid: String,
    client: Option<String>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    validation::mac(&bssid)?;
    if let Some(ref c) = client {
        validation::mac(c)?;
    }

    let mut slot = state.attack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("An attack is already running.".into());
    }
    let (guard, thread) = attack::deauth_loop(interface, bssid, client, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn capture_handshake(
    state: State<'_, AppState>,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    validation::mac(&bssid)?;
    validation::channel(channel)?;

    let mut slot = state.attack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("An attack is already running.".into());
    }
    let prefix = state
        .scans_root
        .join(format!("hs-{}", now()))
        .to_string_lossy()
        .into_owned();
    let (guard, thread) = attack::capture_handshake(interface, bssid, channel, prefix, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn capture_pmkid(
    state: State<'_, AppState>,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    validation::mac(&bssid)?;
    validation::channel(channel)?;

    let mut slot = state.attack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("An attack is already running.".into());
    }
    let prefix = state
        .scans_root
        .join(format!("pmkid-{}", now()))
        .to_string_lossy()
        .into_owned();
    let (guard, thread) = attack::capture_pmkid(interface, bssid, channel, prefix, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn wps_attack(
    state: State<'_, AppState>,
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    validation::interface(&interface)?;
    validation::mac(&bssid)?;
    validation::channel(channel)?;

    let mut slot = state.attack.lock().map_err(|e| e.to_string())?;
    clear_finished(&mut slot);
    if slot.is_some() {
        return Err("An attack is already running.".into());
    }
    let (guard, thread) = attack::wps_attack(interface, bssid, channel, logs)?;
    *slot = Some(Job { guard, thread });
    Ok(())
}

#[tauri::command]
async fn stop_attack(state: State<'_, AppState>) -> Result<(), String> {
    let job = state.attack.lock().map_err(|e| e.to_string())?.take();
    if let Some(job) = job {
        tauri::async_runtime::spawn_blocking(move || stop_job(job))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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

fn shutdown(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        stop_sessions(&handle);
        handle.exit(0);
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
                    || state.crack.lock().map(|s| s.is_some()).unwrap_or(false);
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
