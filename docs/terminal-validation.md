# Terminal migration verification — 2026-10-25

Completed locally on Windows:

- Rust formatting, 20 unit/regression tests, Clippy with warnings denied.
- Optimized executable build (approximately 1.1 MB on Windows).
- Dependency-tree inspection: no desktop framework, GTK, WebKit or Wry dependencies.
- Ratatui TestBackend rendering across all workspaces at 20×8, 40×16, 80×24 and
  120×36, including narrow layout, selected network and confirmation overlay.
- Native terminal smoke test: startup diagnostics, workspace navigation, external
  tool-not-installed error, terminal restoration after handoff failure, clean exit.
- During the local idle/navigation smoke test, Windows working set samples were
  7,114,752 and 7,180,288 bytes. These are not Raspberry Pi/Linux RSS measurements.
- Release shell script passes Bash syntax checking. Linux package construction,
  artifact attestation and GitHub release publication have not run locally.

Local Linux validation was attempted but unavailable: Rustup's ARM Linux standard
library download stalled and was stopped; subsequent target checks reported missing
`core`/`std`. The installed Ubuntu WSL distribution failed to start with
`Wsl/Service/E_UNEXPECTED`. CI now tests on Linux and Windows and builds native ARM64
releases, but those workflows have not yet run for these unpushed changes.

Hardware validation remains necessary for interface changes, Linux foreground
process groups, external Wifite/Kismet sessions and capture workloads. No claim of
verified 1 GB Pi performance is made. See `ratatui-migration.md` for the test matrix.

Run `python3 scripts/measure.py PID 60` on Linux to sample Y5 plus its descendant
processes. CPU percentage uses one CPU core as 100%; memory sums process RSS (shared
pages may be counted more than once). Short-lived children between samples can be
missed, so use a long workload and complement with OS monitoring.
