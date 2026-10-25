//! Foreground terminal handoff keeps external interactive tools usable without a web UI.
use std::{
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub fn run(command: &mut Command, stop: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = crate::process::ManagedChild(command.spawn()?);
    #[cfg(target_os = "linux")]
    let _foreground = Foreground::take(child.id());
    let mut cancelled = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("Tool exited with {status}").into())
            };
        }
        if stop.load(Ordering::Relaxed) {
            if cancelled.is_none() {
                cancelled = Some(Instant::now());
                #[cfg(target_os = "linux")]
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGINT);
                }
            }
            if cancelled.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) {
                #[cfg(target_os = "linux")]
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                child.kill()?;
                child.wait()?;
                return Err("Tool cancelled".into());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
#[cfg(target_os = "linux")]
struct Foreground {
    old: i32,
}
#[cfg(target_os = "linux")]
impl Foreground {
    fn take(pid: u32) -> Self {
        unsafe {
            let old = libc::tcgetpgrp(libc::STDIN_FILENO);
            if old >= 0 {
                let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
                libc::tcsetpgrp(libc::STDIN_FILENO, pid as i32);
                libc::signal(libc::SIGTTOU, previous);
                libc::kill(-(pid as i32), libc::SIGCONT);
            }
            Self { old }
        }
    }
}
#[cfg(target_os = "linux")]
impl Drop for Foreground {
    fn drop(&mut self) {
        if self.old >= 0 {
            unsafe {
                let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
                libc::tcsetpgrp(libc::STDIN_FILENO, self.old);
                libc::signal(libc::SIGTTOU, previous);
            }
        }
    }
}
