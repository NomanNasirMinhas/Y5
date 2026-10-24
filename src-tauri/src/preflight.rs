use crate::{process, wifi};
use serde::Serialize;

#[derive(Serialize)]
pub struct Check {
    name: String,
    ok: bool,
    required: bool,
    detail: String,
}

#[tauri::command]
pub async fn preflight() -> Result<Vec<Check>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut checks = Vec::new();
        for (tool, package, required) in [
            ("iw", "iw", true),
            ("ip", "iproute2", true),
            ("airmon-ng", "aircrack-ng", true),
            ("airodump-ng", "aircrack-ng", true),
            ("aireplay-ng", "aircrack-ng", true),
            ("aircrack-ng", "aircrack-ng", true),
            ("hcxdumptool", "hcxdumptool", false),
            ("hcxpcapngtool", "hcxtools", false),
            ("hashcat", "hashcat", false),
            ("reaver", "reaver", false),
            ("bully", "bully", false),
        ] {
            let path = process::which(tool);
            checks.push(Check {
                name: tool.into(),
                ok: path.is_some(),
                required,
                detail: path.unwrap_or_else(|| format!("Install package: {package}")),
            });
        }
        #[cfg(target_os = "linux")]
        checks.push(Check {
            name: "Privileges".into(),
            ok: unsafe { libc::geteuid() == 0 },
            required: true,
            detail: "Live radio operations require root or appropriate system capabilities.".into(),
        });
        let interfaces = wifi::list_interfaces();
        checks.push(Check {
            name: "Wireless adapter".into(),
            ok: interfaces.as_ref().is_ok_and(|items| !items.is_empty()),
            required: true,
            detail: match interfaces {
                Ok(items) if !items.is_empty() => items
                    .iter()
                    .map(|i| format!("{}: {}", i.name, i.interface_type))
                    .collect::<Vec<_>>()
                    .join(", "),
                Ok(_) => {
                    "No wireless adapter detected. Check USB passthrough, driver and firmware."
                        .into()
                }
                Err(e) => e,
            },
        });
        checks
    })
    .await
    .map_err(|e| e.to_string())
}
