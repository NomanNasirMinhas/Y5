use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

use serde::Deserialize;

use crate::events::Channel;
use crate::model::{now, CrackStatus, LogEvent, Wordlist};
use crate::process::{self, ChildGuard};

/// A hashcat run requested by the frontend.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashcatOptions {
    pub capture: String,
    /// `None` (auto) derives the mode from the capture: 22000 for `.22000`
    /// files and hcxpcapngtool conversions, 2500 for legacy `.hccap`.
    pub hash_mode: Option<u32>,
    /// "wordlist" | "rules" | "mask".
    pub attack: String,
    pub wordlist: Option<String>,
    pub rules: Option<String>,
    pub mask: Option<String>,
    pub increment: bool,
}

struct Prepared {
    hashfile: String,
    mode: u32,
}

pub fn require() -> Result<(), String> {
    if process::which("hashcat").is_none() {
        return Err(process::missing(
            "hashcat",
            "Install hashcat (e.g. `sudo apt install hashcat`, or download it from hashcat.net). \
             A GPU with working drivers is required for acceleration.",
        ));
    }
    Ok(())
}

/// Start hashcat in a worker thread, streaming status and logs.
pub fn hashcat_start(
    opts: HashcatOptions,
    workdir: PathBuf,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    require()?;
    validate(&opts)?;
    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let handle = std::thread::spawn(move || {
        let errors = logs.clone();
        if let Err(message) = run_hashcat(opts, &workdir, status, logs, guard2) {
            let _ = errors.send(LogEvent {
                level: "error".into(),
                message,
                timestamp: now(),
            });
        }
    });
    Ok((guard, handle))
}

fn validate(opts: &HashcatOptions) -> Result<(), String> {
    crate::validation::file(&opts.capture)?;
    if let Some(mode) = opts.hash_mode {
        if mode != 22000 && mode != 2500 {
            return Err("Unsupported hash mode.".into());
        }
    }
    if opts.attack != "mask" {
        crate::validation::file(opts.wordlist.as_deref().unwrap_or(""))?;
    }
    if opts.attack == "rules" {
        crate::validation::file(opts.rules.as_deref().unwrap_or(""))?;
    }
    match opts.attack.as_str() {
        "wordlist" => {
            if opts.wordlist.as_deref().unwrap_or("").trim().is_empty() {
                return Err("Choose a wordlist for a wordlist attack.".into());
            }
        }
        "rules" => {
            if opts.wordlist.as_deref().unwrap_or("").trim().is_empty() {
                return Err("Choose a wordlist for a rules attack.".into());
            }
            if opts.rules.as_deref().unwrap_or("").trim().is_empty() {
                return Err("Choose a rules file for a rules attack.".into());
            }
        }
        "mask" => {
            if opts.mask.as_deref().unwrap_or("").trim().is_empty() {
                return Err("Enter a mask (e.g. `?d?d?d?d?d?d?d?d`) for a mask attack.".into());
            }
        }
        other => return Err(format!("Unknown hashcat attack mode `{other}`.")),
    }
    Ok(())
}

/// Resolve the capture into a hashcat hash file plus hash mode, converting
/// pcaps via hcxpcapngtool (22000, preferred) or aircrack-ng (2500) as needed.
fn prepare(
    capture: &str,
    workdir: &Path,
    forced: Option<u32>,
    guard: &ChildGuard,
) -> Result<Prepared, String> {
    let lower = capture.to_lowercase();
    if lower.ends_with(".22000") || lower.ends_with(".hc22000") {
        if forced == Some(2500) {
            return Err("This capture is a 22000 file; use hash mode AUTO or 22000.".into());
        }
        return Ok(Prepared {
            hashfile: capture.to_string(),
            mode: 22000,
        });
    }
    if lower.ends_with(".hccap") || lower.ends_with(".hccapx") {
        if forced == Some(22000) {
            return Err("This capture is a legacy hccap file; use hash mode AUTO or 2500.".into());
        }
        return Ok(Prepared {
            hashfile: capture.to_string(),
            mode: 2500,
        });
    }

    std::fs::create_dir_all(workdir).map_err(|e| format!("Could not create {workdir:?}: {e}"))?;
    let stem = Path::new(capture)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("capture");

    if forced != Some(2500) && process::which("hcxpcapngtool").is_some() {
        let out = workdir.join(format!("{stem}-{}.22000", now()));
        let out_s = out.to_string_lossy().into_owned();
        let conv = process::run_cancellable("hcxpcapngtool", &["-o", &out_s, capture], guard)
            .map_err(|e| e.to_string())?;
        if !conv.status.success() {
            return Err(combined(&conv));
        }
        return Ok(Prepared {
            hashfile: out_s,
            mode: 22000,
        });
    }
    if process::which("aircrack-ng").is_some() {
        let out = workdir.join(format!("{stem}-{}", now()));
        let out_s = out.to_string_lossy().into_owned();
        let conv = process::run_cancellable("aircrack-ng", &[capture, "-J", &out_s], guard)
            .map_err(|e| e.to_string())?;
        if !conv.status.success() {
            return Err(combined(&conv));
        }
        let hccap = format!("{out_s}.hccap");
        if !Path::new(&hccap).is_file() {
            return Err(format!("aircrack-ng did not write {hccap}."));
        }
        return Ok(Prepared {
            hashfile: hccap,
            mode: 2500,
        });
    }
    Err(format!(
        "Cannot convert `{capture}` for hashcat: install hcxpcapngtool (preferred) or aircrack-ng."
    ))
}

fn run_hashcat(
    opts: HashcatOptions,
    workdir: &Path,
    status: Channel<CrackStatus>,
    logs: Channel<LogEvent>,
    guard: Arc<ChildGuard>,
) -> Result<(), String> {
    let prepared = match prepare(&opts.capture, workdir, opts.hash_mode, &guard) {
        Ok(p) => p,
        Err(e) => {
            let _ = status.send(CrackStatus {
                running: false,
                capture: opts.capture.clone(),
                mode: "hashcat".into(),
                progress: 0.0,
                key: None,
                message: e.clone(),
                speed: None,
                recovered: None,
            });
            return Err(e);
        }
    };

    let attack_num = if opts.attack == "mask" { "3" } else { "0" };
    let potfile = workdir.join("hashcat.pot");
    let outfile = workdir.join(format!("recovered-{}.txt", now()));
    let mut args: Vec<String> = vec![
        "-m".into(),
        prepared.mode.to_string(),
        "-a".into(),
        attack_num.into(),
        "--status".into(),
        "--status-timer=3".into(),
        "--potfile-path".into(),
        potfile.to_string_lossy().into_owned(),
        "--outfile".into(),
        outfile.to_string_lossy().into_owned(),
        "--outfile-format".into(),
        "2".into(),
        prepared.hashfile.clone(),
    ];
    if opts.attack == "mask" {
        args.push(opts.mask.clone().unwrap_or_default());
        if opts.increment {
            args.push("--increment".into());
        }
    } else {
        args.push(opts.wordlist.clone().unwrap_or_default());
        if opts.attack == "rules" {
            args.push("-r".into());
            args.push(opts.rules.clone().unwrap_or_default());
        }
    }

    let mode_label = format!("hashcat m{} a{}", prepared.mode, attack_num);
    let _ = status.send(CrackStatus {
        running: true,
        capture: opts.capture.clone(),
        mode: mode_label.clone(),
        progress: 0.0,
        key: None,
        message: format!("hashcat starting on {}…", prepared.hashfile),
        speed: None,
        recovered: None,
    });

    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(128);
    let mut child = match process::spawn_logged("hashcat", &args, tx) {
        Ok(c) => c,
        Err(e) => {
            let _ = status.send(CrackStatus {
                running: false,
                capture: opts.capture.clone(),
                mode: mode_label.clone(),
                progress: 0.0,
                key: None,
                message: e.to_string(),
                speed: None,
                recovered: None,
            });
            return Err(e.to_string());
        }
    };
    guard.set_pid(child.id());

    let mut progress = 0.0f64;
    let mut speed: Option<String> = None;
    let mut recovered: Option<String> = None;
    let mut live_key: Option<String> = None;
    let mut tail: Vec<String> = Vec::new();

    loop {
        if guard.stop_requested() {
            break;
        }
        let line = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(line) => line,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if let Some(p) = parse_progress(&line) {
            progress = p;
        }
        if let Some(s) = parse_speed(&line) {
            speed = Some(s);
        }
        if let Some(r) = parse_recovered(&line) {
            recovered = Some(r);
        }
        if live_key.is_none() {
            live_key = parse_cracked_plain(&line).map(str::to_string);
        }
        tail.push(line.clone());
        if tail.len() > 4 {
            tail.remove(0);
        }
        let _ = logs.send(LogEvent {
            level: "info".into(),
            message: line.clone(),
            timestamp: now(),
        });
        let _ = status.send(CrackStatus {
            running: true,
            capture: opts.capture.clone(),
            mode: mode_label.clone(),
            progress,
            key: live_key.clone(),
            message: line,
            speed: speed.clone(),
            recovered: recovered.clone(),
        });
    }

    process::reap(&mut child);

    // Truth is the outfile (plaintext-only lines); fall back to the live line
    // and then to the potfile via `--show` (covers runs skipped as already cracked).
    let key = read_first_line(&outfile).or(live_key).or_else(|| {
        if guard.stop_requested() {
            None
        } else {
            show_from_pot(prepared.mode, &prepared.hashfile, &potfile)
        }
    });

    let message = if key.is_some() {
        "Key recovered".to_string()
    } else if guard.stop_requested() {
        "Stopped".to_string()
    } else if tail.is_empty() {
        "hashcat exited without output.".to_string()
    } else {
        format!("Completed without recovering the key. {}", tail.join(" | "))
    };
    let _ = status.send(CrackStatus {
        running: false,
        capture: opts.capture.clone(),
        mode: mode_label,
        progress: 1.0,
        key,
        message,
        speed,
        recovered,
    });
    Ok(())
}

/// `Progress.........: 1234/567890 (0.22%)` → 0.0022
fn parse_progress(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("Progress")?.trim_start_matches('.');
    let rest = rest.strip_prefix(':')?;
    let start = rest.find('(')?;
    let end = rest.find("%)")?;
    let pct: f64 = rest[start + 1..end].trim().parse().ok()?;
    Some(pct / 100.0)
}

/// `Speed.#*.........:   412.3 MH/s` → "412.3 MH/s" (total line only)
fn parse_speed(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Speed.#*")?.trim_start_matches('.');
    let rest = rest.strip_prefix(':')?;
    let value = rest.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// `Recovered........: 1/1 (100.00%) Digests` → "1/1 (100.00%)"
fn parse_recovered(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Recovered")?.trim_start_matches('.');
    let rest = rest.strip_prefix(':')?;
    let end = rest.find(" Digests")?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Extract the plaintext from a hashcat `<hash>:<plain>` line on the live
/// stream. 22000 hashes are colon-separated themselves (`WPA*…:plain`), so the
/// gate on the `WPA*` prefix keeps status lines from being misparsed.
fn parse_cracked_plain(line: &str) -> Option<&str> {
    if !line.starts_with("WPA*") {
        return None;
    }
    let pos = line.rfind(':')?;
    let plain = &line[pos + 1..];
    if plain.is_empty() {
        None
    } else {
        Some(plain)
    }
}

/// Extract the plaintext from `hashcat --show` output (trusted `hash:plain`
/// lines only).
fn parse_pot_plain(text: &str) -> Option<&str> {
    for line in text.lines() {
        if let Some(pos) = line.rfind(':') {
            let plain = line[pos + 1..].trim();
            if !plain.is_empty() {
                return Some(plain);
            }
        }
    }
    None
}

fn read_first_line(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(8192)
        .read_to_string(&mut text)
        .ok()?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

fn show_from_pot(mode: u32, hashfile: &str, potfile: &Path) -> Option<String> {
    if !potfile.is_file() {
        return None;
    }
    let out = process::run_bounded(
        "hashcat",
        &[
            "-m",
            &mode.to_string(),
            "--show",
            "--potfile-path",
            &potfile.to_string_lossy(),
            hashfile,
        ],
    )
    .ok()?;
    parse_pot_plain(&String::from_utf8_lossy(&out.stdout)).map(str::to_string)
}

/// Version + device information for the UI's device panel.
pub fn info() -> Result<String, String> {
    require()?;
    let mut out = String::new();
    if let Ok(v) = process::run_bounded("hashcat", &["--version"]) {
        out.push_str(&combined(&v));
    }
    if let Ok(i) = process::run_bounded("hashcat", &["-I"]) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&combined(&i));
    }
    if out.trim().is_empty() {
        return Err("hashcat produced no output.".into());
    }
    Ok(out)
}

/// Discover hashcat rule files in common system locations plus app-managed
/// directories.
pub fn list_rules(extra: &[PathBuf]) -> Vec<Wordlist> {
    let mut dirs: Vec<PathBuf> = vec![
        "/usr/share/hashcat/rules".into(),
        "/usr/share/doc/hashcat/rules".into(),
        "/opt/hashcat/rules".into(),
    ];
    dirs.extend(extra.iter().cloned());
    if let Some(bin) = process::which("hashcat") {
        if let Some(dir) = Path::new(&bin).parent() {
            dirs.push(dir.join("rules"));
        }
    }
    crate::crack::scan_files(dirs, &["rule"])
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
    fn parses_hashcat_status() {
        let p = parse_progress("Progress.........: 1234/567890 (0.22%)").unwrap();
        assert!((p - 0.0022).abs() < 1e-9);
        assert_eq!(
            parse_progress("Progress.........: 567890/567890 (100.00%)"),
            Some(1.0)
        );
        assert_eq!(parse_progress("Recovered........: 1/1"), None);
        assert_eq!(
            parse_speed("Speed.#*.........:   412.3 MH/s").as_deref(),
            Some("412.3 MH/s")
        );
        // Per-device speed lines (with timing annotations) are ignored.
        assert_eq!(
            parse_speed("Speed.#1.........:     4123 H/s (5.62ms) @ Accel:64"),
            None
        );
        assert_eq!(
            parse_recovered("Recovered........: 1/1 (100.00%) Digests").as_deref(),
            Some("1/1 (100.00%)")
        );
        assert_eq!(
            parse_recovered("Recovered........: 0/1 (0.00%) Salt-Amplifier"),
            None
        );
    }

    #[test]
    fn extracts_hashcat_plain() {
        let line = "WPA*02*aa:bb:cc:dd:ee:ff*aa:bb:cc:dd:ee:ff*essid*6162636465666768696a\
*68696a6b6c6d6e6f7071*72737475767778797a:password123";
        assert_eq!(parse_cracked_plain(line), Some("password123"));
        // Passphrases may contain spaces; the last colon still splits them.
        assert_eq!(
            parse_cracked_plain("WPA*01*a*b*essid*x*y*z:letme in"),
            Some("letme in")
        );
        // Status and banner lines are never mistaken for cracks.
        assert_eq!(parse_cracked_plain("Progress.........: 1/2 (50.00%)"), None);
        assert_eq!(parse_cracked_plain("WPA*01*a*b*essid*x*y*z:"), None);
        // `--show` output is trusted hash:plain, so any line is fair game.
        assert_eq!(
            parse_pot_plain("hash-not-shown\nWPA*00*aa*bb:potkey\n"),
            Some("potkey")
        );
    }
}
