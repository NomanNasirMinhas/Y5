use std::io::{BufReader, Read};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct ManagedChild(pub Child);
impl std::ops::Deref for ManagedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        reap(&mut self.0);
    }
}

/// Shared handle to a spawned child, usable from any thread to request a graceful stop.
pub struct ChildGuard {
    pid: AtomicU32,
    stop: AtomicBool,
}

impl ChildGuard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pid: AtomicU32::new(0),
            stop: AtomicBool::new(false),
        })
    }
    pub fn set_pid(&self, pid: u32) {
        self.pid.store(pid, Ordering::SeqCst);
    }
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    /// Ask the owning worker to stop. Only the owner signals/reaps its child.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Send SIGINT on Linux (so airodump-ng flushes its `.cap`/`.csv` files).
#[cfg(target_os = "linux")]
pub fn signal_term(pid: u32) {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGINT);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}

/// Spawn `program` with `args`, piping stdout and stderr; each line is forwarded to `tx`.
/// The caller owns the returned `Child` for lifecycle management.
pub fn spawn_logged(
    program: &str,
    args: &[String],
    tx: SyncSender<String>,
) -> std::io::Result<ManagedChild> {
    let mut command = Command::new(program);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            forward_output(out, tx);
        });
    }
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            forward_output(err, tx);
        });
    }
    Ok(ManagedChild(child))
}

// Keep draining even when the bounded queue is full. A slow UI must never block
// a child on stdout. Cap unterminated lines and handle carriage-return progress.
fn forward_output(reader: impl Read, tx: SyncSender<String>) {
    let mut reader = BufReader::new(reader);
    let mut chunk = [0u8; 4096];
    let mut line = Vec::with_capacity(4096);
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &chunk[..count] {
            if b == b'\n' || b == b'\r' {
                if !line.is_empty() {
                    if matches!(
                        tx.try_send(String::from_utf8_lossy(&line).into_owned()),
                        Err(std::sync::mpsc::TrySendError::Disconnected(_))
                    ) {
                        return;
                    }
                    line.clear();
                }
            } else if line.len() < 4096 {
                line.push(b);
            }
        }
    }
    if !line.is_empty() {
        let _ = tx.try_send(String::from_utf8_lossy(&line).into_owned());
    }
}

/// Reap a child: SIGINT first (graceful flush), escalating to SIGKILL after a grace period.
pub fn reap(child: &mut Child) {
    // A worker may already have observed and reaped natural process exit.
    // Do not signal its old PID, which the OS can reuse for another process.
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        signal_term(child.id());
        let deadline = Instant::now() + Duration::from_millis(750);
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Run a tool synchronously, returning its combined output and exit status.
pub fn run_cancellable(
    program: &str,
    args: &[&str],
    guard: &ChildGuard,
) -> Result<Output, std::io::Error> {
    run_with_deadline(program, args, Duration::from_secs(120), Some(guard))
}

/// Radio setup and discovery commands must not block the UI indefinitely.
pub fn run_bounded(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    run_with_deadline(program, args, Duration::from_secs(5), None)
}
fn run_with_deadline(
    program: &str,
    args: &[&str],
    timeout: Duration,
    guard: Option<&ChildGuard>,
) -> Result<Output, std::io::Error> {
    let mut command = Command::new(program);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut child = ManagedChild(child);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    for (is_stdout, mut stream) in [
        (true, Box::new(stdout) as Box<dyn Read + Send>),
        (false, Box::new(stderr) as Box<dyn Read + Send>),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            let result = (|| -> std::io::Result<Vec<u8>> {
                loop {
                    let n = stream.read(&mut chunk)?;
                    if n == 0 {
                        break;
                    }
                    let keep = n.min((1024 * 1024usize).saturating_sub(bytes.len()));
                    bytes.extend_from_slice(&chunk[..keep]);
                }
                Ok(bytes)
            })();
            let _ = tx.send((is_stdout, result));
        });
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if guard.is_some_and(ChildGuard::stop_requested) {
            reap(&mut child);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Operation cancelled",
            ));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            reap(&mut child);
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "{program} did not finish within {} seconds",
                    timeout.as_secs()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    for _ in 0..2 {
        let (is_stdout, bytes) = rx.recv_timeout(Duration::from_secs(1)).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Tool output pipe did not close",
            )
        })?;
        if is_stdout {
            output.stdout = bytes?;
        } else {
            output.stderr = bytes?;
        }
    }
    Ok(output)
}

/// Locate a binary on `PATH`, returning `None` when absent.
pub fn which(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(target_os = "windows")]
        {
            let exe = dir.join(format!("{program}.exe"));
            if exe.is_file() {
                return Some(exe.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Error string for a missing required tool.
pub fn missing(program: &str, install_hint: &str) -> String {
    format!("`{program}` was not found. {install_hint}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_flood_does_not_block_and_caps_lines() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let data = format!("{}\n{}\nlast\r", "a".repeat(100_000), "b".repeat(100_000));
        forward_output(data.as_bytes(), tx);
        assert_eq!(rx.recv().unwrap().len(), 4096);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn cancelled_conversion_returns_promptly() {
        let guard = ChildGuard::new();
        guard.request_stop();
        #[cfg(target_os = "windows")]
        let (program, args) = (
            "powershell.exe",
            vec!["-NoProfile", "-Command", "Start-Sleep 30"],
        );
        #[cfg(not(target_os = "windows"))]
        let (program, args) = ("sleep", vec!["30"]);
        let started = Instant::now();
        let err = run_cancellable(program, &args, &guard).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
