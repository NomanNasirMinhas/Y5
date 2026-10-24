use crate::model::WirelessInterface;
use crate::process;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    Macos,
}

pub fn platform() -> Platform {
    if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Macos
    }
}

pub fn list_interfaces() -> Result<Vec<WirelessInterface>, String> {
    match platform() {
        Platform::Linux => list_linux(),
        Platform::Windows => list_windows(),
        Platform::Macos => list_macos(),
    }
}

fn list_linux() -> Result<Vec<WirelessInterface>, String> {
    let mut ifaces = if let Ok(out) = process::run_bounded("iw", &["dev"]) {
        if out.status.success() {
            parse_iw_dev(&String::from_utf8_lossy(&out.stdout))
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    if ifaces.is_empty() {
        // Fall back to `ip`; keep only wireless-looking names.
        if let Ok(out) = process::run_bounded("ip", &["-o", "link", "show"]) {
            if out.status.success() {
                ifaces = parse_ip_link(&String::from_utf8_lossy(&out.stdout));
            }
        }
    }
    let states = process::run_bounded("ip", &["-o", "link", "show"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_ip_state(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    for i in ifaces.iter_mut() {
        if i.state.is_empty() {
            i.state = states
                .get(&i.name)
                .cloned()
                .unwrap_or_else(|| "unknown".into());
        }
    }
    Ok(ifaces)
}

fn list_windows() -> Result<Vec<WirelessInterface>, String> {
    let out = process::run_bounded("netsh", &["wlan", "show", "interfaces"])
        .map_err(|e| e.to_string())?;
    Ok(parse_netsh(&String::from_utf8_lossy(&out.stdout)))
}

fn list_macos() -> Result<Vec<WirelessInterface>, String> {
    // Best-effort: report the Wi-Fi hardware port and its MAC via `ifconfig`.
    let mut out = Vec::new();
    if let Ok(ports) = process::run_bounded("networksetup", &["-listallhardwareports"]) {
        let text = String::from_utf8_lossy(&ports.stdout).into_owned();
        if let Some(dev) = wifi_device(&text) {
            let mut mac = String::new();
            if let Ok(cfg) = process::run_bounded("ifconfig", &[&dev]) {
                let cfg = String::from_utf8_lossy(&cfg.stdout).into_owned();
                mac = cfg
                    .lines()
                    .find_map(|l| l.split_whitespace().nth(1))
                    .filter(|t| t.contains(':') && t.len() >= 17)
                    .unwrap_or_default()
                    .to_string();
            }
            out.push(WirelessInterface {
                name: dev,
                mac,
                interface_type: "managed".into(),
                state: "up".into(),
            });
        }
    }
    Ok(out)
}

fn wifi_device(text: &str) -> Option<String> {
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim().starts_with("Hardware Port:") && line.contains("Wi-Fi") {
            for next in lines.by_ref() {
                if let Some(dev) = next.split("Device:").nth(1) {
                    return Some(dev.trim().to_string());
                }
                if next.trim().is_empty() {
                    break;
                }
            }
        }
    }
    None
}

/// Parse `iw dev` output into interface records (name, MAC, type).
fn parse_iw_dev(text: &str) -> Vec<WirelessInterface> {
    let mut out = Vec::new();
    for block in text.split("Interface ").skip(1) {
        let mut name = block.lines().next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let mut mac = String::new();
        let mut kind = String::new();
        for line in block.lines() {
            if let Some(v) = line.trim().strip_prefix("addr ") {
                mac = v.trim().to_string();
            } else if let Some(v) = line.trim().strip_prefix("type ") {
                kind = v.trim().to_string();
            }
        }
        // `iw dev` groups interfaces under a phy; trim any trailing "phy#N".
        if let Some(idx) = name.find('\n') {
            name.truncate(idx);
        }
        out.push(WirelessInterface {
            name,
            mac,
            interface_type: if kind.is_empty() {
                "unknown".into()
            } else {
                kind
            },
            state: String::new(),
        });
    }
    out
}

fn wireless_name(name: &str) -> bool {
    name.starts_with("wlan")
        || name.starts_with("wlp")
        || name.starts_with("wl")
        || name.starts_with("mon")
}

/// Parse `ip -o link show` into wireless interface records.
fn parse_ip_link(text: &str) -> Vec<WirelessInterface> {
    let mut out = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(name_token) = tokens.get(1) else {
            continue;
        };
        let name = name_token.trim_end_matches(':').to_string();
        if !wireless_name(&name) {
            continue;
        }
        let mac = tokens
            .iter()
            .position(|t| *t == "link/ether")
            .and_then(|i| tokens.get(i + 1))
            .copied()
            .unwrap_or("")
            .to_string();
        let state = tokens
            .iter()
            .position(|t| *t == "state")
            .and_then(|i| tokens.get(i + 1))
            .copied()
            .unwrap_or("unknown")
            .to_string();
        let interface_type = if name.contains("mon") {
            "monitor"
        } else {
            "managed"
        };
        out.push(WirelessInterface {
            name,
            mac,
            interface_type: interface_type.to_string(),
            state,
        });
    }
    out
}

/// Parse `ip -o link show` into a name → state map.
fn parse_ip_state(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let _ = fields.next();
        if let Some(name_token) = fields.next() {
            let name = name_token.trim_end_matches(':').to_string();
            if let Some(pos) = line.find(" state ") {
                let rest = &line[pos + " state ".len()..];
                let state = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                map.insert(name, state);
            }
        }
    }
    map
}

/// Parse `netsh wlan show interfaces` into interface records.
fn parse_netsh(text: &str) -> Vec<WirelessInterface> {
    let mut out = Vec::new();
    let mut name = String::new();
    let mut mac = String::new();
    let mut state = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_string();
            match k.trim() {
                "Name" => name = v,
                "Physical address" => mac = v,
                "State" => state = v,
                _ => {}
            }
        }
        if !name.is_empty() && !mac.is_empty() && line.starts_with("State") {
            out.push(WirelessInterface {
                name: std::mem::take(&mut name),
                mac: std::mem::take(&mut mac),
                interface_type: "managed".into(),
                state: std::mem::take(&mut state),
            });
        }
    }
    if !name.is_empty() {
        out.push(WirelessInterface {
            name,
            mac,
            interface_type: "managed".into(),
            state,
        });
    }
    out
}

fn set_mode(interface: &str, mode: &str) -> Result<String, String> {
    if platform() != Platform::Linux {
        return Err("Radio mode changes require Linux.".into());
    }
    crate::validation::interface(interface)?;
    require("iw")?;
    require("ip")?;
    let info =
        process::run_bounded("iw", &["dev", interface, "info"]).map_err(|e| e.to_string())?;
    if !info.status.success() {
        return Err(combined(&info));
    }
    if String::from_utf8_lossy(&info.stdout)
        .lines()
        .any(|line| line.trim() == format!("type {mode}"))
    {
        return Ok(interface.into());
    }
    let down = process::run_bounded("ip", &["link", "set", interface, "down"])
        .map_err(|e| e.to_string())?;
    if !down.status.success() {
        return Err(combined(&down));
    }
    let change = process::run_bounded("iw", &["dev", interface, "set", "type", mode]);
    // Bring the device back up even if the mode change fails.
    let up = process::run_bounded("ip", &["link", "set", interface, "up"]);
    let change = change.map_err(|e| e.to_string())?;
    if !change.status.success() {
        return Err(combined(&change));
    }
    let up = up.map_err(|e| e.to_string())?;
    if !up.status.success() {
        return Err(combined(&up));
    }
    let check =
        process::run_bounded("iw", &["dev", interface, "info"]).map_err(|e| e.to_string())?;
    if !check.status.success()
        || !String::from_utf8_lossy(&check.stdout)
            .lines()
            .any(|line| line.trim() == format!("type {mode}"))
    {
        return Err(format!("Could not verify {mode} mode on {interface}. Check driver and network-service conflicts."));
    }
    Ok(interface.into())
}

pub fn start_monitor(interface: &str) -> Result<String, String> {
    set_mode(interface, "monitor")
}
pub fn stop_monitor(interface: &str) -> Result<String, String> {
    set_mode(interface, "managed").map(|name| format!("{name}: managed mode"))
}

/// Kill interfering processes (NetworkManager, wpa_supplicant, …) that block monitor mode.
pub fn check_kill() -> Result<String, String> {
    if platform() != Platform::Linux {
        return Err("check-kill requires Linux.".into());
    }
    require("airmon-ng")?;
    let out = process::run_bounded("airmon-ng", &["check", "kill"]).map_err(|e| e.to_string())?;
    Ok(combined(&out))
}

pub fn require(program: &str) -> Result<(), String> {
    if process::which(program).is_none() {
        let hint = if program == "airmon-ng" || program == "airodump-ng" || program == "aircrack-ng"
        {
            "Install aircrack-ng (e.g. `sudo apt install aircrack-ng`)."
        } else if program == "iw" {
            "Install `iw` (e.g. `sudo apt install iw`)."
        } else {
            "Ensure the tool is installed and on PATH."
        };
        return Err(process::missing(program, hint));
    }
    Ok(())
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

pub fn set_channel(interface: &str, channel: i32) -> Result<(), String> {
    let result = process::run_bounded(
        "iw",
        &["dev", interface, "set", "channel", &channel.to_string()],
    )
    .map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(combined(&result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iw_dev() {
        let sample = "\
phy#0
\tInterface wlan0
\t\tifindex 3
\t\twdev 0x1
\t\taddr aa:bb:cc:dd:ee:ff
\t\ttype managed
phy#1
\tInterface wlan0mon
\t\tifindex 4
\t\taddr 11:22:33:44:55:66
\t\ttype monitor
";
        let ifaces = parse_iw_dev(sample);
        assert_eq!(ifaces.len(), 2);
        assert_eq!(ifaces[0].name, "wlan0");
        assert_eq!(ifaces[0].interface_type, "managed");
        assert_eq!(ifaces[0].mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(ifaces[1].name, "wlan0mon");
        assert_eq!(ifaces[1].interface_type, "monitor");
    }

    #[test]
    fn parses_ip_link() {
        let sample = "\
2: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DORMANT group default qlen 1000\\    link/ether aa:bb:cc:dd:ee:ff brd ff:ff:ff:ff:ff:ff
3: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP group default qlen 1000\\    link/ether 00:11:22:33:44:55 brd ff:ff:ff:ff:ff:ff
";
        let ifaces = parse_ip_link(sample);
        assert_eq!(ifaces.len(), 1);
        assert_eq!(ifaces[0].name, "wlan0");
        assert_eq!(ifaces[0].state, "UP");
        assert_eq!(ifaces[0].mac, "aa:bb:cc:dd:ee:ff");
    }
}
