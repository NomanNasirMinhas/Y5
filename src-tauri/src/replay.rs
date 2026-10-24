use std::path::Path;

use crate::model::CaptureInfo;

/// List crackable captures (`.cap`/`.pcap`/`.pcapng`/`.22000`/`.hccap`/`.hccapx`),
/// newest first. Works on every platform — no live interface required.
pub fn list(dir: &Path) -> Result<Vec<CaptureInfo>, String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(
            ext.as_str(),
            "cap" | "pcap" | "pcapng" | "22000" | "hccap" | "hccapx"
        ) {
            continue;
        }
        if let Ok(meta) = p.metadata() {
            let created = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64 * 1000)
                .unwrap_or(0);
            out.push(CaptureInfo {
                path: p.to_string_lossy().into_owned(),
                name: p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                bytes: meta.len(),
                created,
            });
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.created));
    Ok(out)
}
