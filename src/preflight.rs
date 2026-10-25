use crate::{process, wifi};
pub fn check() -> Vec<String> {
    let mut lines = vec![format!(
        "Y5 {} | {} | Pi low-memory profile",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    )];
    for (tool, required) in [
        ("iw", true),
        ("ip", true),
        ("airodump-ng", true),
        ("aireplay-ng", false),
        ("aircrack-ng", false),
        ("hcxpcapngtool", false),
        ("hcxdumptool", false),
        ("hashcat", false),
        ("reaver", false),
        ("bully", false),
        ("wifite", false),
        ("kismet", false),
        ("tshark", false),
        ("tcpdump", false),
    ] {
        lines.push(format!(
            "[{}] {tool} {}",
            if process::which(tool).is_some() {
                "OK"
            } else {
                "--"
            },
            if required {
                "(live scan)"
            } else {
                "(optional)"
            }
        ));
    }
    #[cfg(target_os = "linux")]
    lines.push(
        if unsafe { libc::geteuid() } == 0 {
            "[OK] root privileges"
        } else {
            "[--] Live radio operations need root; offline work remains available"
        }
        .into(),
    );
    match wifi::list_interfaces() {
        Ok(items) => {
            for iface in items {
                lines.push(format!("[OK] {} {}", iface.name, iface.interface_type));
            }
        }
        Err(e) => lines.push(format!("[--] {e}")),
    }
    lines
}
