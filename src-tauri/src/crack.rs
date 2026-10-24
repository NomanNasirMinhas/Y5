use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::model::{now, CrackStatus, LogEvent, Wordlist};
use crate::process::{self, ChildGuard};
use crate::wifi;
use tauri::ipc::Channel;

/// Start aircrack-ng in a worker thread, streaming status and logs.
pub fn crack_start(
    capture: String,
    wordlist: String,
    bssid: Option<String>,
    mode: String,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    wifi::require("aircrack-ng")?;
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let errors = logs.clone();
        if let Err(message) = run_crack(capture, wordlist, bssid, mode, status, logs, guard2) {
            let _ = errors.send(LogEvent {
                level: "error".into(),
                message,
                timestamp: now(),
            });
        }
    });
    Ok((guard, handle))
}

fn run_crack(
    capture: String,
    wordlist: String,
    bssid: Option<String>,
    mode: String,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
    guard: Arc<ChildGuard>,
) -> Result<(), String> {
    let mut args = vec![capture.clone()];
    args.push("-w".to_string());
    args.push(wordlist.clone());
    if let Some(b) = bssid {
        args.push("-b".to_string());
        args.push(b);
    }

    let _ = status.send(CrackStatus {
        running: true,
        capture: capture.clone(),
        mode: mode.clone(),
        progress: 0.0,
        key: None,
        message: "Starting…".to_string(),
        speed: None,
        recovered: None,
    });

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut child = process::spawn_logged("aircrack-ng", &args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());

    let mut found: Option<String> = None;
    let mut last = String::new();
    loop {
        if guard.stop_requested() {
            break;
        }
        let line = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(line) => line,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if let Some(k) = extract_key(&line) {
            found = Some(k);
        }
        last = line.clone();
        let _ = logs.send(LogEvent {
            level: "info".into(),
            message: line.clone(),
            timestamp: now(),
        });
        let _ = status.send(CrackStatus {
            running: true,
            capture: capture.clone(),
            mode: mode.clone(),
            progress: 0.0,
            key: None,
            message: line,
            speed: None,
            recovered: None,
        });
    }

    process::reap(&mut child);
    let _ = status.send(CrackStatus {
        running: false,
        capture: capture.clone(),
        mode: mode.clone(),
        progress: 1.0,
        key: found.clone(),
        message: match &found {
            Some(_) => "Key recovered".to_string(),
            None if guard.stop_requested() => "Stopped".to_string(),
            None => format!("Completed without finding the key. {last}"),
        },
        speed: None,
        recovered: None,
    });
    Ok(())
}

/// Extract a recovered key from an aircrack-ng `KEY FOUND! [ … ]` line.
fn extract_key(line: &str) -> Option<String> {
    if !line.contains("KEY FOUND!") {
        return None;
    }
    let start = line.find('[')? + 1;
    let end = start + line[start..].find(']')?;
    Some(line[start..end].trim().to_string())
}

/// Export a WPA capture to hashcat format (`.hccap` via aircrack-ng, or `.22000` via hcxpcapngtool).
pub fn export_hashcat(capture: &str) -> Result<String, String> {
    wifi::require("aircrack-ng")?;
    let hccap = format!("{capture}.hccap");
    let out = process::run("aircrack-ng", &[capture, "-J", &hccap]).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(combined(&out));
    }
    let mut message = format!("Wrote {hccap} (hashcat mode 2500).");
    if process::which("hcxpcapngtool").is_some() {
        let hc22000 = format!("{capture}.22000");
        let convert =
            process::run("hcxpcapngtool", &["-o", &hc22000, capture]).map_err(|e| e.to_string())?;
        if convert.status.success() {
            message = format!("Wrote {hccap} (mode 2500) and {hc22000} (mode 22000).");
        }
    }
    Ok(message)
}

/// Discover wordlists in common system locations plus app-managed directories.
pub fn list_wordlists(extra: &[PathBuf]) -> Vec<Wordlist> {
    let mut dirs: Vec<PathBuf> = vec![
        "/usr/share/wordlists".into(),
        "/usr/share/dict".into(),
        "/usr/share/seclists/Passwords".into(),
    ];
    dirs.extend(extra.iter().cloned());
    scan_files(dirs, &["txt", "lst", "gz", "dic", "dict"])
}

/// Collect files with one of `exts` from `dirs`, sorted by name.
pub fn scan_files(dirs: Vec<PathBuf>, exts: &[&str]) -> Vec<Wordlist> {
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !exts.contains(&ext) {
                continue;
            }
            if let Ok(meta) = p.metadata() {
                out.push(Wordlist {
                    path: p.to_string_lossy().into_owned(),
                    name: p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    bytes: meta.len(),
                });
            }
        }
    }
    out.sort_by_key(|w| w.name.to_lowercase());
    out
}

fn combined(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !e.is_empty() {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(&e);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_wpa_key() {
        assert_eq!(
            extract_key("KEY FOUND! [ password123 ]"),
            Some("password123".into())
        );
        assert_eq!(extract_key("no key here"), None);
    }

    #[test]
    fn extracts_wep_key() {
        assert_eq!(
            extract_key("KEY FOUND! [ AA:BB:CC:DD:EE ]"),
            Some("AA:BB:CC:DD:EE".into())
        );
    }
}
