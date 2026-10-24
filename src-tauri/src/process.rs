use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
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
pub fn spawn_logged(program: &str, args: &[String], tx: Sender<String>) -> std::io::Result<Child> {
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
pub fn run(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

/// Radio setup and discovery commands must not block the UI indefinitely.
pub fn run_bounded(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
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
            let result = stream.read_to_end(&mut bytes).map(|_| bytes);
            let _ = tx.send((is_stdout, result));
        });
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{program} did not finish within 5 seconds"),
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
