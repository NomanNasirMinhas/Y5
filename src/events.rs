use crate::model::{CrackStatus, LogEvent, ScanSnapshot};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Typed delivery independent of a UI framework; producers never wait for rendering.
pub struct Channel<T>(Arc<dyn Fn(T) -> Result<(), String> + Send + Sync>);
impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Channel<T> {
    pub fn new(f: impl Fn(T) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
    pub fn send(&self, value: T) -> Result<(), String> {
        (self.0)(value)
    }
}
#[derive(Default)]
pub struct Inbox {
    pub logs: VecDeque<LogEvent>,
    pub snapshot: Option<ScanSnapshot>,
    pub status: Option<CrackStatus>,
    pub dropped: u64,
    pub failed: bool,
}
pub type Bus = Arc<Mutex<Inbox>>;
pub fn logs(bus: &Bus) -> Channel<LogEvent> {
    let bus = bus.clone();
    Channel::new(move |mut event: LogEvent| {
        event.message = event
            .message
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .take(2048)
            .collect();
        let mut inbox = bus.lock().map_err(|e| e.to_string())?;
        if event.level == "error" {
            inbox.failed = true;
        }
        if inbox.logs.len() == 512 {
            inbox.logs.pop_front();
            inbox.dropped += 1;
        }
        inbox.logs.push_back(event);
        Ok(())
    })
}
pub fn snapshots(bus: &Bus) -> Channel<ScanSnapshot> {
    let bus = bus.clone();
    Channel::new(move |mut event: ScanSnapshot| {
        event.networks.truncate(1024);
        event.clients.truncate(2048);
        for n in &mut event.networks {
            n.essid.retain(|c| !c.is_control());
        }
        bus.lock().map_err(|e| e.to_string())?.snapshot = Some(event);
        Ok(())
    })
}
pub fn statuses(bus: &Bus) -> Channel<CrackStatus> {
    let bus = bus.clone();
    Channel::new(move |event| {
        bus.lock().map_err(|e| e.to_string())?.status = Some(event);
        Ok(())
    })
}
pub fn report(bus: &Bus, level: &str, message: impl Into<String>) {
    let _ = logs(bus).send(LogEvent {
        level: level.into(),
        message: message.into(),
        timestamp: crate::model::now(),
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flood_is_bounded_and_keeps_latest() {
        let bus = Bus::default();
        for i in 0..10000 {
            report(&bus, "info", i.to_string());
        }
        let inbox = bus.lock().unwrap();
        assert_eq!(inbox.logs.len(), 512);
        assert_eq!(inbox.dropped, 9488);
        assert_eq!(inbox.logs.back().unwrap().message, "9999");
    }
}
