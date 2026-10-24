# Y5 interface explorations

Generated using the built-in imagegen tool. These images explore layout and color; illustrative versions, dates, vendors and checklist contents are not product claims.

- `graphite-amber.png`: charcoal instrument panels, off-white text, amber accents, side navigation.
- `slate-cyan.png`: blue slate panels, cyan accents, horizontal navigation, wider workspace.

The current implementation uses a neutral graphite/amber palette with its existing responsive layout. Neither image represents a fully implemented replacement layout.

## Behavior changes

- Scan stop no longer waits for inherited output pipes to close; late running snapshots after stop are ignored.
- Scan results support single or multiple selection. Review selected targets transfers BSSID and channel to the attack workspace; choose and run one target at a time.
- Startup diagnostics list core and optional tools, privileges and detected interface modes. Browser preview does not simulate successful hardware checks.
- Radio setup commands have time limits. Monitor-mode requests detect an already configured interface; managed-mode recovery verifies the result.
- Normal close and the manual recovery control stop jobs and restore radios used in the session to managed mode. Recovery errors keep the window open. Forced termination, VM shutdown and crashes cannot run this cleanup; use Interfaces to recover after restarting.

Linux hardware validation is still required: scan/stop/restart on the Alfa adapter, close while scanning, already-monitor startup, managed recovery, disconnected USB and missing-tool cases. Automated checks on Windows cannot validate Linux drivers or radio state changes.

## Implemented slate interface and automatic radio lifecycle

The selected slate/cyan design is now implemented, with working top tabs, responsive cards, network selection, diagnostics, recovery toggle, scan duration and activity filtering. The app shows actual available fields (clients rather than an unimplemented vendor lookup) and labels browser tool checks as preview-only.

Starting a scan or target job prepares the selected interface in place with iw/ip. Jobs return it to managed mode on completion, cancellation or startup failure. Radio setup and recovery run on a supervisor thread; stop commands signal cancellation immediately, while the UI stays in Stopping until cleanup finishes. Status polling never blocks on a worker-held lock. A silent-subprocess cancellation regression checks that attack stopping does not depend on output arriving. Linux child tools get their own process group for coordinated termination. The app prevents overlapping radio jobs during setup, execution and cleanup.

The exit preference controls fallback cleanup for manually configured session radios; normal job completion always restores managed mode. Driver or permission errors are surfaced rather than reported as successful recovery. Linux hardware validation remains necessary.
