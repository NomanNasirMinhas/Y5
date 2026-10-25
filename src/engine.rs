use crate::{
    attack, capture, crack,
    events::{self, Bus},
    hashcat,
    process::ChildGuard,
    validation, wifi,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
    time::Duration,
};

pub struct Job {
    pub name: String,
    pub guard: Arc<ChildGuard>,
    pub thread: JoinHandle<()>,
}
pub struct Engine {
    pub bus: Bus,
    pub root: PathBuf,
    pub job: Option<Job>,
    _lock: std::fs::File,
}
impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            job.guard.request_stop();
            let _ = job.thread.join();
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub action: String,
    pub interface: String,
    pub bssid: String,
    pub channel: i32,
    pub capture: String,
    pub wordlist: String,
    pub rules: String,
    pub mask: String,
    pub count: u32,
    pub client: String,
    pub mode: String,
    pub increment: bool,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            action: "handshake".into(),
            interface: String::new(),
            bssid: String::new(),
            channel: 1,
            capture: String::new(),
            wordlist: String::new(),
            rules: String::new(),
            mask: String::new(),
            count: 5,
            client: String::new(),
            mode: "auto".into(),
            increment: false,
        }
    }
}
pub fn data_root() -> PathBuf {
    if let Some(path) = std::env::var_os("Y5_DATA_DIR") {
        return path.into();
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .unwrap_or_else(|| ".".into()),
            )
            .join(".local/share")
        });
    // Preserve the prior application's data location without copying large captures.
    let legacy = base.join("local.y5.desktop");
    if legacy.exists() {
        legacy
    } else {
        base.join("y5")
    }
}
impl Engine {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        for dir in ["scans", "wordlists", "rules"] {
            std::fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("session.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "Another Y5 session uses this data directory".to_string())?;
        Ok(Self {
            bus: Bus::default(),
            root,
            job: None,
            _lock: lock,
        })
    }
    pub fn busy(&mut self) -> bool {
        if self.job.as_ref().is_some_and(|j| j.thread.is_finished()) {
            if let Some(j) = self.job.take() {
                if j.thread.join().is_err() {
                    events::report(
                        &self.bus,
                        "error",
                        "Worker panicked; use recovery before another radio job",
                    );
                }
            }
        }
        self.job.is_some()
    }
    pub fn stop(&self) {
        if let Some(j) = &self.job {
            j.guard.request_stop();
            events::report(
                &self.bus,
                "info",
                "Stop requested; waiting for process cleanup and interface recovery",
            );
        }
    }
    pub fn start(&mut self, request: Request) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the active job and its cleanup to finish".into());
        }
        validate(&request)?;
        self.bus.lock().map_err(|e| e.to_string())?.failed = false;
        let journal = self.root.join("recovery.json");
        if journal.exists() && request.action != "recover" {
            return Err("Unfinished radio recovery: press R before starting another job".into());
        }
        let guard = ChildGuard::new();
        let worker_guard = guard.clone();
        let bus = self.bus.clone();
        let root = self.root.clone();
        let name = request.action.clone();
        let thread = std::thread::spawn(move || {
            let radio = matches!(
                request.action.as_str(),
                "scan" | "handshake" | "pmkid" | "wps" | "deauth"
            );
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
                    if radio {
                        for tool in match request.action.as_str() {
                            "scan" => vec!["iw", "ip", "airodump-ng"],
                            "handshake" => vec!["iw", "ip", "airodump-ng", "aireplay-ng"],
                            "pmkid" => vec!["iw", "ip", "hcxdumptool"],
                            "deauth" => vec!["iw", "ip", "aireplay-ng"],
                            _ => vec!["iw", "ip"],
                        } {
                            wifi::require(tool)?;
                        }
                        if request.action == "wps"
                            && crate::process::which("reaver").is_none()
                            && crate::process::which("bully").is_none()
                        {
                            return Err("Install reaver or bully for WPS".into());
                        }
                        if fs2::available_space(&root).map_err(|e| e.to_string())?
                            < 128 * 1024 * 1024
                        {
                            return Err("At least 128 MiB of free disk is required".into());
                        }
                        record_radio(&journal, &request.interface)?;
                        events::report(
                            &bus,
                            "info",
                            format!("Preparing {} for monitor mode", request.interface),
                        );
                        wifi::start_monitor(&request.interface)?;
                    }
                    if worker_guard.stop_requested() {
                        return Ok(());
                    }
                    let logs = events::logs(&bus);
                    let status = events::statuses(&bus);
                    let prefix = root
                        .join("scans")
                        .join(format!("{}-{}", request.action, crate::model::now()))
                        .to_string_lossy()
                        .into_owned();
                    let launched = match request.action.as_str() {
                        "scan" => capture::start(
                            request.interface.clone(),
                            None,
                            prefix,
                            events::snapshots(&bus),
                            logs,
                        ),
                        "handshake" => attack::capture_handshake(
                            request.interface.clone(),
                            request.bssid.clone(),
                            request.channel,
                            prefix,
                            logs,
                        ),
                        "pmkid" => attack::capture_pmkid(
                            request.interface.clone(),
                            request.bssid.clone(),
                            request.channel,
                            prefix,
                            logs,
                        ),
                        "wps" => attack::wps_attack(
                            request.interface.clone(),
                            request.bssid.clone(),
                            request.channel,
                            logs,
                        ),
                        "deauth" => {
                            wifi::set_channel(&request.interface, request.channel)?;
                            attack::deauth_job(
                                request.interface.clone(),
                                request.bssid.clone(),
                                if request.client.is_empty() {
                                    None
                                } else {
                                    Some(request.client.clone())
                                },
                                request.count,
                                logs,
                            )
                        }
                        "aircrack" => crack::crack_start(
                            request.capture.clone(),
                            request.wordlist.clone(),
                            if request.bssid.is_empty() {
                                None
                            } else {
                                Some(request.bssid.clone())
                            },
                            request.mode.clone(),
                            status,
                            logs,
                        ),
                        "hashcat" => hashcat::hashcat_start(
                            hashcat::HashcatOptions {
                                capture: request.capture.clone(),
                                hash_mode: None,
                                attack: if !request.mask.is_empty() {
                                    "mask"
                                } else if !request.rules.is_empty() {
                                    "rules"
                                } else {
                                    "wordlist"
                                }
                                .into(),
                                wordlist: Some(request.wordlist.clone()),
                                rules: if request.rules.is_empty() {
                                    None
                                } else {
                                    Some(request.rules.clone())
                                },
                                mask: if request.mask.is_empty() {
                                    None
                                } else {
                                    Some(request.mask.clone())
                                },
                                increment: request.increment,
                            },
                            root.join("scans/hashcat"),
                            status,
                            logs,
                        ),
                        "export" => {
                            events::report(
                                &bus,
                                "info",
                                crack::export_hashcat(&request.capture, &worker_guard)?,
                            );
                            return Ok(());
                        }
                        "devices" => {
                            events::report(&bus, "info", hashcat::info()?);
                            return Ok(());
                        }
                        "recover" => {
                            recover(&journal)?;
                            events::report(
                                &bus,
                                "info",
                                "Recorded interface restored to managed mode",
                            );
                            return Ok(());
                        }
                        _ => return Err("Unknown workflow".into()),
                    }?;
                    events::report(&bus, "info", format!("{} running", request.action));
                    let mut storage_check = std::time::Instant::now();
                    while !launched.1.is_finished() {
                        if worker_guard.stop_requested() {
                            launched.0.request_stop();
                        }
                        if radio && storage_check.elapsed() >= Duration::from_secs(2) {
                            storage_check = std::time::Instant::now();
                            if fs2::available_space(&root)
                                .is_ok_and(|bytes| bytes < 128 * 1024 * 1024)
                            {
                                events::report(
                                    &bus,
                                    "error",
                                    "Less than 128 MiB free disk space; stopping capture",
                                );
                                launched.0.request_stop();
                            }
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    launched
                        .1
                        .join()
                        .map_err(|_| "Worker panicked".to_string())?;
                    Ok(())
                }))
                .unwrap_or_else(|_| Err("Job panicked".into()));
            if let Err(e) = result {
                events::report(&bus, "error", e);
            }
            if radio {
                events::report(&bus, "info", "Restoring managed mode");
                if let Err(e) = recover(&journal) {
                    events::report(
                        &bus,
                        "error",
                        format!("Recovery failed: {e}. Journal retained; press R to retry."),
                    );
                }
            }
            events::report(&bus, "info", "Job finished");
        });
        self.job = Some(Job {
            name,
            guard,
            thread,
        });
        Ok(())
    }
}
pub fn recover(journal: &Path) -> Result<(), String> {
    if !journal.exists() {
        return Ok(());
    }
    let raw = std::fs::read(journal).map_err(|e| e.to_string())?;
    let record: Recovery = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let iface = match record {
        Recovery::Legacy(name) => name,
        Recovery::Adapter { name, mac } => {
            let interfaces = wifi::list_interfaces()?;
            interfaces
                .iter()
                .find(|i| i.name == name)
                .or_else(|| interfaces.iter().find(|i| !mac.is_empty() && i.mac == mac))
                .map(|i| i.name.clone())
                .ok_or_else(|| {
                    format!("Recorded adapter {name} is disconnected; reconnect it and retry")
                })?
        }
    };
    validation::interface(&iface)?;
    wifi::stop_monitor(&iface)?;
    std::fs::remove_file(journal).map_err(|e| e.to_string())
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum Recovery {
    Legacy(String),
    Adapter { name: String, mac: String },
}
pub fn record_radio(journal: &Path, iface: &str) -> Result<(), String> {
    validation::interface(iface)?;
    let interfaces = wifi::list_interfaces()?;
    let adapter = interfaces
        .iter()
        .find(|i| i.name == iface)
        .ok_or("Selected wireless interface is unavailable")?;
    let bytes = serde_json::to_vec(&Recovery::Adapter {
        name: iface.into(),
        mac: adapter.mac.clone(),
    })
    .map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut file = std::fs::File::create(journal).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
fn validate(r: &Request) -> Result<(), String> {
    if ![
        "scan",
        "handshake",
        "pmkid",
        "wps",
        "deauth",
        "aircrack",
        "hashcat",
        "export",
        "devices",
        "recover",
    ]
    .contains(&r.action.as_str())
    {
        return Err("Unknown workflow".into());
    }
    if !r.client.is_empty() {
        validation::mac(&r.client)?;
    }
    if !["auto", "wpa", "wep"].contains(&r.mode.as_str()) {
        return Err("Recovery mode must be auto, wpa or wep".into());
    }
    if matches!(
        r.action.as_str(),
        "scan" | "handshake" | "pmkid" | "wps" | "deauth"
    ) {
        validation::interface(&r.interface)?;
    }
    if matches!(r.action.as_str(), "handshake" | "pmkid" | "wps" | "deauth") {
        validation::mac(&r.bssid)?;
        validation::channel(r.channel)?;
    }
    if r.action == "deauth" && r.count > 9999 {
        return Err("Count must be 0 (continuous) through 9999".into());
    }
    if matches!(r.action.as_str(), "aircrack" | "hashcat" | "export") {
        validation::file(&r.capture)?;
    }
    if r.action == "aircrack" || (r.action == "hashcat" && r.mask.is_empty()) {
        validation::file(&r.wordlist)?;
    }
    if !r.rules.is_empty() {
        validation::file(&r.rules)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_targets_are_rejected_before_launch() {
        let r = Request {
            interface: "wlan0".into(),
            bssid: "not-a-mac".into(),
            ..Request::default()
        };
        assert!(validate(&r).is_err());
        assert!(validate(&Request {
            action: "unknown".into(),
            ..Request::default()
        })
        .is_err());
    }
    #[test]
    fn lock_and_invalid_recovery_preserve_state() {
        let root = std::env::temp_dir().join(format!("y5-engine-test-{}", std::process::id()));
        let engine = Engine::new(root.clone()).unwrap();
        assert!(Engine::new(root.clone()).is_err());
        let journal = root.join("recovery.json");
        std::fs::write(&journal, br#""bad;interface""#).unwrap();
        assert!(recover(&journal).is_err());
        assert!(journal.exists());
        drop(engine);
        std::fs::remove_dir_all(root).unwrap();
    }
}
