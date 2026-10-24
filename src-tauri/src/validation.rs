pub fn interface(value: &str) -> Result<(), String> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(|c| c.is_control()) {
        return Err("Choose a valid interface.".into());
    }
    Ok(())
}

pub fn mac(value: &str) -> Result<(), String> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 6
        || parts
            .iter()
            .any(|p| p.len() != 2 || !p.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err("Invalid MAC address. Expected AA:BB:CC:DD:EE:FF.".into());
    }
    Ok(())
}

pub fn channel(value: i32) -> Result<(), String> {
    if !(1..=165).contains(&value) {
        return Err("Channel must be from 1 to 165.".into());
    }
    Ok(())
}

pub fn file(value: &str) -> Result<(), String> {
    if value.starts_with('-') || !std::path::Path::new(value).is_file() {
        return Err(format!("Input file does not exist: {value}"));
    }
    std::fs::File::open(value).map_err(|e| format!("Cannot read input file: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_inputs() {
        assert!(mac("AA:bb:00:11:22:33").is_ok());
        assert!(mac("--help").is_err());
        assert!(mac("AA:BB:CC:DD:EE:GG").is_err());
        assert!(interface("").is_err());
        assert!(interface("--help").is_err());
        assert!(channel(0).is_err());
        assert!(channel(166).is_err());
        assert!(channel(36).is_ok());
        assert!(file("--help").is_err());
        assert!(file("/nonexistent-y5-capture.cap").is_err());
    }
}
