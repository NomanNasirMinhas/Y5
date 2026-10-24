use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::model::{now, Client, LogEvent, Network, ScanSnapshot};
use crate::process::{self, ChildGuard};
use crate::wifi;
use tauri::ipc::Channel;

/// Spawn airodump-ng in a worker thread, streaming snapshots and log lines over
/// `Channel`s. Returns a `(guard, handle)` pair so callers can request a stop.
pub fn start(
    interface: String,
    channel: Option<i32>,
    prefix: String,
    snapshots: Channel<ScanSnapshot>,
    logs: Channel<LogEvent>,
) -> Result<(Arc<ChildGuard>, JoinHandle<()>), String> {
    if wifi::platform() != wifi::Platform::Linux {
        return Err("Live scanning requires Linux with a compatible wireless chipset.".into());
    }
    wifi::require("airodump-ng")?;

    let guard = ChildGuard::new();
    let guard2 = guard.clone();
    let prefix_path = prefix.clone();
    let interface2 = interface.clone();
    let handle = std::thread::spawn(move || {
        let errors = logs.clone();
        if let Err(message) = run(interface2, channel, prefix_path, guard2, snapshots, logs) {
            let _ = errors.send(LogEvent {
                level: "error".into(),
                message,
                timestamp: now(),
            });
        }
    });
    Ok((guard, handle))
}

fn run(
    interface: String,
    channel: Option<i32>,
    prefix: String,
    guard: Arc<ChildGuard>,
    snapshots: Channel<ScanSnapshot>,
    logs: Channel<LogEvent>,
) -> Result<(), String> {
    let mut args = vec![interface.clone()];
    args.push("-w".to_string());
    args.push(prefix.clone());
    args.push("--write-interval".to_string());
    args.push("1".to_string());
    args.push("--band".to_string());
    args.push("abg".to_string());
    if let Some(ch) = channel {
        args.push("--channel".to_string());
        args.push(ch.to_string());
    }

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut child = process::spawn_logged("airodump-ng", &args, tx).map_err(|e| e.to_string())?;
    guard.set_pid(child.id());

    // Forward airodump's terminal output as info logs (handshake notices, etc.).
    let log_chan = logs.clone();
    let log_reader = std::thread::spawn(move || {
        while let Ok(line) = rx.recv() {
            let _ = log_chan.send(LogEvent {
                level: "info".into(),
                message: line,
                timestamp: now(),
            });
        }
    });

    let mut first_net: HashMap<String, i64> = HashMap::new();
    let mut first_cli: HashMap<String, i64> = HashMap::new();
    let fixed = channel.unwrap_or(0);
    let prefix_base = prefix.clone();

    loop {
        let t = now();
        let (mut networks, mut clients) = read_csvs(&prefix_base);
        for n in networks.iter_mut() {
            let seen = first_net.entry(n.bssid.clone()).or_insert(t);
            n.first_seen = *seen;
            n.last_seen = t;
        }
        for c in clients.iter_mut() {
            let seen = first_cli.entry(c.mac.clone()).or_insert(t);
            c.first_seen = *seen;
            c.last_seen = t;
        }
        let msg = if fixed > 0 {
            format!("Locked to channel {fixed}")
        } else {
            "Hopping 2.4/5 GHz channels".to_string()
        };
        let _ = snapshots.send(ScanSnapshot {
            running: true,
            interface: interface.clone(),
            channel: fixed,
            networks,
            clients,
            message: msg,
        });

        // Poll the stop flag for ~1s before refreshing.
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if guard.stop_requested() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if guard.stop_requested() || matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
    }

    process::reap(&mut child);
    let _ = log_reader.join();

    // Final snapshot so the UI can show the last seen state and clear "running".
    let t = now();
    let (mut networks, mut clients) = read_csvs(&prefix_base);
    for n in networks.iter_mut() {
        n.last_seen = t;
    }
    for c in clients.iter_mut() {
        c.last_seen = t;
    }
    let _ = snapshots.send(ScanSnapshot {
        running: false,
        interface,
        channel: fixed,
        networks,
        clients,
        message: "Scan stopped".to_string(),
    });
    Ok(())
}

/// Read the airodump CSV files for `prefix`, returning networks and clients.
fn read_csvs(prefix: &str) -> (Vec<Network>, Vec<Client>) {
    let nets = read_to_string(&format!("{prefix}-01.csv"))
        .map(|s| parse_networks(&s))
        .unwrap_or_default();
    let clients = read_to_string(&format!("{prefix}-01.csv"))
        .map(|s| parse_clients(&s))
        .unwrap_or_default();
    (nets, clients)
}

fn read_to_string(path: &str) -> Option<String> {
    std::fs::read_to_string(Path::new(path)).ok()
}

/// Parse the airodump-ng AP CSV (`.csv`) into networks.
pub fn parse_networks(text: &str) -> Vec<Network> {
    let mut nets = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Station MAC") {
            break;
        }
        if line.is_empty() || line.starts_with("BSSID") {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 14 {
            continue;
        }
        let bssid = fields[0].trim().to_string();
        if bssid.len() < 17 {
            continue;
        }
        let channel: i32 = fields[3].trim().parse().unwrap_or(0);
        let privacy = fields[5].trim().to_string();
        let cipher = fields[6].trim().to_string();
        let auth = fields[7].trim().to_string();
        let power: i32 = fields[8].trim().parse().unwrap_or(0);
        let beacons: u64 = fields[9].trim().parse().unwrap_or(0);
        let data: u64 = fields[10].trim().parse().unwrap_or(0);
        // ESSID sits at index 13; the trailing "Key" column is last.
        let key_idx = fields.len() - 1;
        let essid = fields[13..key_idx].join(",").trim().to_string();
        nets.push(Network {
            bssid,
            essid,
            channel,
            privacy,
            cipher,
            auth,
            power,
            beacons,
            data_packets: data,
            first_seen: 0,
            last_seen: 0,
            wps: false,
        });
    }
    nets
}

/// Parse the station section of airodump-ng's combined AP/station CSV.
pub fn parse_clients(text: &str) -> Vec<Client> {
    let mut clients = Vec::new();
    let mut stations = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Station MAC") {
            stations = true;
            continue;
        }
        if !stations || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 6 {
            continue;
        }
        let mac = fields[0].trim().to_string();
        if mac.len() < 17 {
            continue;
        }
        let power: i32 = fields[3].trim().parse().unwrap_or(0);
        let packets: u64 = fields[4].trim().parse().unwrap_or(0);
        let bssid = fields[5].trim().to_string();
        let probed = fields[6..].join(",").trim().to_string();
        clients.push(Client {
            mac,
            bssid,
            power,
            packets,
            probed_essids: probed,
            first_seen: 0,
            last_seen: 0,
        });
    }
    clients
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_access_points_and_stations_in_combined_csv() {
        let sample = "BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\nAA:BB:CC:DD:EE:FF, t, t, 6, 130, WPA2, CCMP, PSK, -45, 100, 0, 0, 4, Test,\n\nStation MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs\n11:22:33:44:55:66, t, t, -50, 20, AA:BB:CC:DD:EE:FF, Test\n";
        assert_eq!(parse_networks(sample).len(), 1);
        let clients = parse_clients(sample);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].packets, 20);
        assert_eq!(clients[0].bssid, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn parses_networks() {
        let sample = "\
BSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key
AA:BB:CC:DD:EE:FF, 2026-01-01 00:00:00, 2026-01-01 00:01:00, 6, 130, WPA2, CCMP, PSK, -45, 100, 0, 0.0.0.0, 5, MyNetwork,
11:22:33:44:55:66, 2026-01-01 00:00:00, 2026-01-01 00:01:00, 11, 54, OPN, , , -70, 50, 0, 0.0.0.0, 8, OpenNet,
";
        let nets = parse_networks(sample);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(nets[0].essid, "MyNetwork");
        assert_eq!(nets[0].channel, 6);
        assert_eq!(nets[0].privacy, "WPA2");
        assert_eq!(nets[0].power, -45);
        assert_eq!(nets[1].essid, "OpenNet");
        assert_eq!(nets[1].privacy, "OPN");
    }

    #[test]
    fn parses_clients() {
        let sample = "\
Station MAC, First time seen, Last time seen, Power, # packets, BSSID, Probed ESSIDs
AA:AA:AA:AA:AA:AA, 2026-01-01 00:00:00, 2026-01-01 00:01:00, -30, 20, AA:BB:CC:DD:EE:FF,
BB:BB:BB:BB:BB:BB, 2026-01-01 00:00:00, 2026-01-01 00:01:00, -50, 5, (not associated), MyNetwork
";
        let clients = parse_clients(sample);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].mac, "AA:AA:AA:AA:AA:AA");
        assert_eq!(clients[0].bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(clients[1].probed_essids, "MyNetwork");
    }
}
