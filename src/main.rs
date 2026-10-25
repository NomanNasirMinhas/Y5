mod attack;
mod capture;
mod crack;
mod engine;
mod events;
mod hashcat;
mod model;
mod preflight;
mod process;
mod replay;
mod terminal_job;
mod ui;
mod validation;
mod wifi;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn main() {
    if let Err(e) = run() {
        eprintln!("Y5: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::umask(0o077);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") && !args.iter().any(|s| s == "tool") {
        println!("Y5 terminal Wi-Fi console\n\ny5                         Interactive console\ny5 doctor                  Dependency checks\ny5 recover                 Restore journaled interface\ny5 --interface wlan0 tool wifite -- <args>\ny5 tool hashcat -- <args>    Full external tool terminal handoff\n\nY5_DATA_DIR overrides data storage. Linux required for radio workflows.");
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "--version") {
        println!(
            "y5 {}{}",
            env!("CARGO_PKG_VERSION"),
            option_env!("Y5_BUILD_NUMBER")
                .map(|n| format!("+build.{n}"))
                .unwrap_or_default()
        );
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "doctor") {
        for line in preflight::check() {
            println!("{line}");
        }
        return Ok(());
    }
    let mut engine = engine::Engine::new(engine::data_root())?;
    if args.first().is_some_and(|s| s == "wordlists") {
        for file in crack::list_wordlists(&[engine.root.join("wordlists")]) {
            println!("{}", file.path);
        }
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "rules") {
        for file in hashcat::list_rules(&[engine.root.join("rules")]) {
            println!("{}", file.path);
        }
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "check-kill") {
        println!("{}", wifi::check_kill()?);
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "recover") {
        engine::recover(&engine.root.join("recovery.json"))?;
        println!("Recovery complete");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    if let Some(index) = args.iter().position(|s| s == "tool") {
        return handoff(&engine, &args, index, stop);
    }
    if !args.is_empty() {
        return Err("Unknown argument; use --help".into());
    }
    let result = ui::run(&mut engine, stop);
    engine.stop();
    while engine.busy() {
        std::thread::sleep(Duration::from_millis(50));
    }
    result?;
    if engine.root.join("recovery.json").exists() {
        return Err("Interface recovery remains pending; run y5 recover".into());
    }
    Ok(())
}
fn handoff(
    engine: &engine::Engine,
    args: &[String],
    index: usize,
    stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tool = args.get(index + 1).ok_or("Missing tool name")?;
    if ![
        "wifite",
        "kismet",
        "hashcat",
        "tshark",
        "tcpdump",
        "aircrack-ng",
        "hcxpcapngtool",
        "reaver",
        "bully",
    ]
    .contains(&tool.as_str())
    {
        return Err("Unsupported tool; see README".into());
    }
    wifi::require(tool)?;
    let journal = engine.root.join("recovery.json");
    if journal.exists() {
        return Err("Run y5 recover before another tool".into());
    }
    let iface = args[..index]
        .iter()
        .position(|s| s == "--interface")
        .and_then(|i| args.get(i + 1));
    if let Some(iface) = iface {
        validation::interface(iface)?;
        engine::record_radio(&journal, iface)?;
    }
    let mut cmd = std::process::Command::new(tool);
    let tail = &args[index + 2..];
    cmd.args(if tail.first().is_some_and(|s| s == "--") {
        &tail[1..]
    } else {
        tail
    });
    let result = terminal_job::run(&mut cmd, stop);
    if iface.is_some() {
        engine::recover(&journal)?;
    }
    result
}
