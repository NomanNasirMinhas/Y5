# Y5 — Airwave Terminal Console

A Linux-first Wi-Fi audit console written in Rust with **Ratatui + Crossterm**, designed for local terminals and SSH on a Raspberry Pi 4 with 1 GB RAM. The desktop/WebView stack has been removed.

Use radio workflows only on networks you own or are authorized to test.

## Build and run

```bash
cargo build --release --locked
./target/release/y5 doctor
sudo ./target/release/y5
```

Build on a stronger machine or use a CI package when possible; compiling on a 1 GB Pi can itself require substantial memory. A Pi running a 64-bit OS needs **ARM64**, not AMD64.

```bash
uname -m  # aarch64 on a 64-bit Pi OS
sudo apt install aircrack-ng iw iproute2
```

Optional tools are detected individually: hcxdumptool, hcxtools, hashcat, reaver/bully, wifite, kismet, tshark and tcpdump. Install the packages needed for your workflow. Offline browsing/recovery does not require a wireless adapter. Live radio operations require Linux, appropriate privileges, and compatible driver/firmware.

## Terminal flow

Choose an adapter in Interfaces → press `s` to scan → select networks → stop with `x` → configure a workflow in Targets → press `g` to review and run.

| Key | Action |
| --- | --- |
| Tab / Shift+Tab / 1–7 | Switch workspace |
| Up / Down | Navigate rows or form fields |
| Enter | Select one network, select adapter/capture, or edit a field |
| Space | Toggle network selection for a sequential queue |
| `/` | Filter networks |
| `v` on Networks | Switch between networks and associated clients |
| Left / Right on Targets | Choose workflow |
| `s` / `x` | Start scan / stop current job and clear queue |
| `r` / `R` | Refresh diagnostics / recover recorded interface |
| `g` | Review/run a workflow, or hand over the terminal on Tools |
| `q` / Ctrl+C | Stop, restore the interface, then exit |

The console has Overview, Networks, Targets, Captures, Interfaces, Tools and Logs workspaces. It supports small terminals, with a compact network layout. Field editing uses Enter to save and Esc to cancel. Errors appear in Logs; optional tools are not started at launch.

Native workflows include passive survey/client discovery, handshake and PMKID capture, WPS, deauthentication, Aircrack recovery, Hashcat wordlist/rules/mask recovery, conversion/export and Hashcat device diagnostics. Selected radio targets run sequentially. Hashcat status, recovered results and a persistent potfile remain available.

## Optional tool integrations

Tools can hand the terminal to Wifite, Kismet, Hashcat, Tshark, Tcpdump, Aircrack, hcxtools, Reaver or Bully, then return to Y5. Enter arguments as a JSON string array, such as `["--help"]`; arguments are passed directly, never through a shell. For radio tools, select an adapter in Interfaces and also pass that adapter using the tool's own arguments.

Equivalent command-line handoffs:

```bash
y5 tool hashcat -- --help
y5 --interface wlan0 tool wifite -- --help
y5 tool kismet -- --help
y5 wordlists
y5 rules
y5 recover
```

`--interface` records the adapter for recovery; it is not substituted into upstream arguments. `y5 check-kill` explicitly runs airmon-ng's service-stop operation; it is never run automatically and may disconnect networking.

Wifite/Kismet retain their upstream interfaces. Y5 does not independently reimplement every feature, and Kismet's device/alert API panels are not yet native TUI screens. See the [capability matrix and limitations](docs/ratatui-migration.md).

## Low-memory behavior

No GUI runtime or background web server is needed. Rendering is event-driven and capped at 5 FPS. Log buffers, process output, scan snapshots and CSV reads are bounded. Native jobs run one at a time; stop requests do not wait on rendering. Native radio jobs enable monitor mode and restore managed mode on stop/completion. A persistent journal supports recovery after interruption.

Hashcat and Kismet consume resources of their own. On a 1 GB Pi, capture locally and export hashes for a stronger computer when appropriate. No Raspberry Pi RAM/CPU numbers have yet been measured for this version; the resource targets are documented, not claimed as achieved.

## Data and upgrades

`Y5_DATA_DIR` overrides storage. By default, Y5 uses `$XDG_DATA_HOME/y5` or `~/.local/share/y5`. If the previous Linux data folder `local.y5.desktop` exists under that base, it is reused without copying captures. Launch under the same user as before, or set the old data path explicitly when using sudo. Scans, wordlists and rules retain their subdirectories.

The desktop updater is removed. Install terminal updates explicitly using `.deb` packages or replace the standalone binary after verifying its checksum and GitHub provenance:

```bash
gh attestation verify y5_0.2.0+build.N_arm64.deb -R NomanNasirMinhas/Y5
sudo apt install ./y5_0.2.0+build.N_arm64.deb
```

Replace N with the downloaded release build number. The command above applies to a newly published terminal artifact, not earlier desktop releases. CI publishes prereleases only from `build`, after Rust tests and lint checks. Packages include SHA256 checksums and GitHub artifact attestations; the old updater signing secrets are no longer used.

| CI architecture | Artifacts | Build baseline |
| --- | --- | --- |
| AMD64 | `.deb`, `.tar.gz` | Ubuntu 22.04 / glibc 2.35 |
| ARM64 (Pi 4, 64-bit OS) | `.deb`, `.tar.gz` | Ubuntu 24.04 / glibc 2.39 |

The DEB records its glibc requirement. Older distributions need a source build on a compatible baseline. ARMv7 packaging is not yet enabled. No desktop session, AppImage, Node, GTK or WebKit is required.

## Development checks

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

Tests cover parsers, target validation, bounded log/output handling, cancellation and TUI layouts. Actual adapter switching, Linux process-group cleanup, external terminal handoffs and 1 GB Pi performance require the [hardware validation checklist](docs/ratatui-migration.md).
