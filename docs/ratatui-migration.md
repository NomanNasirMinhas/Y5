# Terminal migration

Y5 0.2 replaces the desktop frontend with Ratatui/Crossterm. There is no browser,
Node runtime, GUI server, GTK/WebKit dependency or desktop updater in the build.
Old screenshots under `docs/prototypes` document the retired desktop design.

## Capability matrix

| Capability | Implementation |
| --- | --- |
| Passive discovery and associated clients | Native terminal screens, airodump-ng backend |
| Single/multiple target selection | Enter selects one; Space selects multiple; sequential queue |
| Handshake/PMKID/WPS/deauthentication | Existing tool adapters, cancellation and interface recovery |
| Offline Aircrack recovery | Auto/WPA/WEP mode, wordlist and optional target |
| Hashcat | Wordlist, rules, mask/increment, status, recovered key, device info and potfile |
| Capture export | Prefer hcxtools mode 22000; legacy Aircrack fallback |
| Wifite | Full upstream interactive terminal handoff; Y5 does not duplicate every workflow |
| Kismet | On-demand upstream process/terminal handoff; its web/API device and alert panels remain upstream |
| Tshark/tcpdump/Reaver/Bully | Optional terminal handoff with explicit arguments |
| Session restore for external Hashcat | Through upstream `--session` / `--restore` arguments |
| Native Hashcat pause/resume controls | Not yet exposed as TUI buttons |
| Automatic desktop updates | Removed; explicit package replacement with provenance verification |

There is no claim of independently reimplementing every feature of every Wi-Fi
tool. The Tools workspace preserves access to upstream interfaces, including
features not exposed in Y5's native forms. External dependencies must be installed
separately; missing optional tools do not block discovery or offline browsing.

## Resource policy

- One native job at a time on the default Pi profile; targets execute sequentially.
- State-driven rendering, limited to five draws per second. Idle event wait: 100 ms.
- 512 pending log lines and 512 displayed lines; text capped at 2,048 characters.
- Tool pipes keep draining under load, with 128 queued lines of at most 4 KiB.
  Excess pipe output is dropped; subprocess status and final output files remain
  the source of truth. Terminal control characters are removed from displayed logs.
- Only the latest scan/recovery snapshot is retained. CSV input is capped at 2 MiB;
  displayed data is capped at 1,024 networks and 2,048 clients. Very dense surveys
  may therefore omit entries. Capture files remain available for external analysis.
- Diagnostic output is capped at 1 MiB per stream; diagnostics time out after five
  seconds. Conversion commands have a 120-second deadline and accept cancellation.
- Native radio jobs stop when free disk falls below 128 MiB, checked every two seconds.
  This is a free-space guard, not a total capture retention quota. Delete old captures
  explicitly; no captured data is automatically deleted.
- No automatic Kismet or Hashcat startup. Their own memory/CPU usage is additional;
  Y5 cannot make compute-heavy password recovery inexpensive on a 1 GB board.

Targets of under 40 MiB idle RSS and under 1% idle CPU are **not verified Pi results**.
Measure Y5 and its child process tree separately on the actual Pi before promoting
the terminal prerelease. Resource limits on arbitrary external handoff commands
are controlled by those tools/the OS, not silently imposed by Y5.

## Recovery

Before native radio setup, Y5 records the interface name/MAC and flushes the journal
to disk. Native jobs enable monitor mode, then return the adapter to managed mode
after completion, cancellation or worker errors. Recovery can resolve a renamed
interface by MAC. Failed restoration preserves the journal and prevents new jobs.
Press `R` or run `y5 recover` after reconnecting the adapter. This does not restart
network services previously stopped manually or by an external tool.

`q`, Ctrl+C, SIGINT and SIGTERM request cleanup. SIGKILL/power loss cannot run cleanup;
the journal supports recovery on the next launch. Sessions sharing a data directory
are locked; do not run separate data directories against the same radio.

The Tools workspace temporarily restores the normal terminal and hands its input
to the selected program. Exiting that program returns to Y5. Radio handoffs journal
the selected adapter but do not rewrite upstream interface arguments; pass the same
adapter to the tool. Standalone CLI handoffs should use `--interface IFACE` before
`tool` for recovery. Upstream tools that change MAC addresses and names together may
need manual restoration; the journal is kept rather than guessing another adapter.

## Validation on the target Pi

1. Confirm `uname -m` reports `aarch64` and `ldd --version` meets the package baseline.
2. Run `y5 doctor`; test disconnected and connected USB adapter states.
3. Scan an owned lab network, stop, and verify `iw dev` reports managed mode.
4. Repeat stop during startup, rapid stop/restart, capture completion, adapter unplug,
   terminal resize and SSH disconnect. Confirm no orphaned processes remain.
5. Test interrupted recovery and `y5 recover`, including adapter renaming.
6. Run a long scan and a pipe-output flood; record RSS/CPU for the whole process tree.
7. Verify Hashcat device detection and export before attempting local recovery.
8. Test Kismet/Wifite handoff and return; confirm terminal state and adapter recovery.

Windows tests cover parsing, cancellation, bounded delivery and terminal rendering;
they do not validate the Alfa driver, Linux signals/TTY handoff or RF behavior.
