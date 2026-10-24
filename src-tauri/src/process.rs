use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

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
    /// Ask the worker to stop and nudge the child with SIGINT so blocking reads return.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let pid = self.pid.load(Ordering::SeqCst);
        if pid != 0 {
            signal_term(pid);
        }
    }
}

/// Send SIGINT on Linux (so airodump-ng flushes its `.cap`/`.csv` files).
pub fn signal_term(pid: u32) {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::kill(pid as i32, libc::SIGINT);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}

/// Spawn `program` with `args`, piping stdout and stderr; each line is forwarded to `tx`.
/// The caller owns the returned `Child` for lifecycle management.
pub fn spawn_logged(program: &str, args: &[String], tx: Sender<String>) -> std::io::Result<Child> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    Ok(child)
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
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            if Instant::now() >= deadline {
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
pub fn run(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
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
