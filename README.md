# Y5

A local-first WiFi security auditing dashboard built with **Tauri v2 + React**. It consolidates the workflows of **wifite** (automation), **kismet** (passive discovery), **airmon-ng** (monitor-mode management) and the **aircrack-ng** suite (deauth, handshake capture, WEP/WPA cracking) into one desktop app — shelling out to a system-installed aircrack-ng suite, the same way RF Radar shells out to its RTL-SDR driver.

> **Authorized testing only.** Deauthentication and cracking disrupt wireless networks and can recover keys. Use Y5 only on networks you own, or where you have explicit written permission to test.

## Features

| Workspace | What it does |
| --- | --- |
| **Scan** | Passive airodump-ng survey — live networks (BSSID, ESSID, channel, encryption, signal, client count) and associated stations. |
| **Attacks** | Deauth (aireplay-ng), WPA handshake capture, PMKID capture (hcxdumptool), WPS recovery (reaver/bully). |
| **Crack** | Offline recovery with two engines: **aircrack-ng** (wordlist) and **hashcat** (wordlist, wordlist + rules, or mask attacks) with live progress, speed and recovered-digest readouts — plus hashcat export (`.hccap` / `.22000`). |
| **Interfaces** | List wireless interfaces and toggle monitor mode (airmon-ng), check-kill. |
| **Log** | Unified stream of scan / attack / crack output. |

## Platform reality

- **Linux — full suite.** Monitor mode and packet injection require Linux with a compatible chipset (Atheros, Ralink or Realtek). This is where scanning, deauth, handshake/PMKID capture and WPS work.
- **Windows / macOS — degraded source builds.** Offline cracking of existing captures and passive listing where the OS permits it. No monitor mode or injection. Automated releases currently provide Linux installers only.

## Prerequisites

Y5 does **not** bundle the aircrack-ng or hashcat binaries. Install them system-wide:

- **Linux (Debian/Ubuntu):** `sudo apt install aircrack-ng iw`
  - Optional: `hcxdumptool` + `hcxpcapngtool` (PMKID), `reaver` or `bully` (WPS).
- **Windows:** Install [Aircrack-ng](https://www.aircrack-ng.org/) and [hashcat](https://hashcat.net/hashcat/) (or the WSL path) and add them to `PATH`. Cracking works; injection does not.
- **macOS:** `brew install aircrack-ng hashcat` — cracking only; injection does not.

### hashcat engine

The Crack workspace can run hashcat in-app (wordlist, wordlist + rules, or mask attacks) against captured handshakes. Y5 detects a system-installed `hashcat` at startup:

- **Install:** `sudo apt install hashcat` (or see [hashcat.net](https://hashcat.net/hashcat/)).
- **GPU acceleration** needs working OpenCL/CUDA drivers — on Debian/Ubuntu: `sudo apt install ocl-icd-opencl-dev` plus your vendor's driver. Without them hashcat falls back to CPU and warns; Y5 never passes `--force`, so driver problems surface instead of being masked.
- **Capture conversion** is automatic: `.22000` files run as mode 22000, `.hccap`/`.hccapx` as mode 2500, and raw `.cap`/`.pcap` captures are converted with `hcxpcapngtool` (mode 22000) or, if that's absent, `aircrack-ng -J` (mode 2500). Install `hcxtools` for the best path: `sudo apt install hcxtools`.
- **Rules** are picked up from `/usr/share/hashcat/rules`, hashcat's install directory, and the app's own `rules/` data folder (shown alongside captures/wordlists in-app).
- **Potfile** lives in the app's data directory, so recovered keys are remembered between sessions and shown instantly via `hashcat --show`.

For a compatible adapter, Atheros AR9271 (e.g. Alfa AWUS036NHA), Ralink RT3070 and Realtek RTL8812AU chipsets are common picks.

## Development

```bash
npm ci
npm run tauri -- dev          # desktop app (Linux for the full suite)
npm run dev                   # browser demo with synthetic data
npm test                      # Vitest (TS helpers)
npm run check                 # UI regressions, production build, release helper checks
npm run check:native          # Rust formatting, tests, Clippy
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The browser preview (`npm run dev`) renders synthetic networks so the UI can be explored without a radio; attack and crack paths explain that they require the desktop app.

### Verification

Run `npm run check` and `npm run check:native` before and after changes. The
Project checks workflow runs these on pushes and pull requests; the release
workflow also verifies each platform before packaging.

Regression coverage includes scan start/stop, rejected stops, completion events
arriving during startup, recovery results, target validation, combined AP/client
CSV parsing, and completed worker cleanup. Runtime checks reject invalid target
addresses, numeric ranges and missing input files. Job polling reconciles workers
that exit without a terminal status event; failed stops keep controls available.

For UI changes, check all workspaces at desktop, tablet and phone widths, including
horizontal table scrolling, keyboard navigation, the guide dialog and reduced
motion. Browser checks use synthetic data. Actual monitor mode, capture, injection
and recovery still require a separate authorized hardware test on the supported
platform with the required tools installed.

## Building installers

```bash
npm run tauri -- build        # bundles for the current platform
```

CI (`push` to the `build` branch, or a manual run on that branch) currently publishes **Linux only**:

| Architecture | Downloads | Intended systems |
| --- | --- | --- |
| x86_64 / AMD64 | `.deb`, `.AppImage` | Debian, Ubuntu, Kali Linux, Parrot OS, and other compatible Linux desktops |
| ARM64 / aarch64 | `.deb`, `.AppImage` | Raspberry Pi with a 64-bit desktop OS (Raspberry Pi OS Bookworm or newer), Kali ARM64, Ubuntu ARM64, and compatible ARM64 desktops |

Choose the architecture of your installed OS. The same package serves compatible
distributions; there are no separate Kali or Parrot builds. DEB packages install
`aircrack-ng`, `iw`, and `xdg-utils` dependencies; AppImage users must install the
wireless tools separately. Both architectures include signed updater artifacts
and checksums, and the updater manifest requires all four Linux packages.

Builds use Ubuntu 22.04 as the compatibility baseline and need a graphical desktop
and the Tauri v2 Linux runtime. AppImage portability does not guarantee every Linux
distribution or older glibc release. 32-bit ARMv7 / Raspberry Pi OS and headless
systems are not supported by these installers. Windows and macOS release jobs are
disabled for now; their source build support remains available.

### Updater signing

The updater needs a keypair. Generate one once, publish the public key into `src-tauri/tauri.conf.json`, and store the private key as a GitHub Actions secret:

```bash
npx @tauri-apps/cli signer generate -w ~/.tauri/y5.key
# The printed public key goes into src-tauri/tauri.conf.json -> plugins.updater.pubkey.
```

Then add repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY` — contents of the generated key file.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — its password (if set).

## Project layout

```
src/                 React frontend (App shell, Scan/Attacks/Crack/Interfaces/Log tabs, bridge + demo fallback)
src-tauri/src/       Rust backend (process orchestration, wifi, capture, attack, crack, hashcat, replay, updater)
scripts/ci/          Release metadata, asset collection, manifest generation + self-checks
.github/workflows/   Build-and-release matrix
```

## Credits

Architecture, stack and CI conventions mirror [RF Radar](https://github.com/NomanNasirMinhas/RF-Radar). Y5 is for educational and authorized security testing.
