use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirelessInterface {
    pub name: String,
    pub mac: String,
    pub interface_type: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub bssid: String,
    pub essid: String,
    pub channel: i32,
    pub privacy: String,
    pub cipher: String,
    pub auth: String,
    pub power: i32,
    pub beacons: u64,
    pub data_packets: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub wps: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub mac: String,
    pub bssid: String,
    pub power: i32,
    pub packets: u64,
    pub probed_essids: String,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSnapshot {
    pub running: bool,
    pub interface: String,
    pub channel: i32,
    pub networks: Vec<Network>,
    pub clients: Vec<Client>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub level: String,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInfo {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub created: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wordlist {
    pub path: String,
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrackStatus {
    pub running: bool,
    pub capture: String,
    pub mode: String,
    pub progress: f64,
    pub key: Option<String>,
    pub message: String,
    /// Engine-specific extras (hashcat): last reported speed and recovered/total.
    pub speed: Option<String>,
    pub recovered: Option<String>,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
