import React, { useEffect, useState } from "react";
import { Wifi, Play, Square, Radio, Search, Info } from "lucide-react";
import { listInterfaces, startScan, stopScan, platform, native } from "./bridge";
import { useJobCompletion } from "./useJobCompletion";
import {
  WirelessInterface,
  ScanSnapshot,
  LogEvent,
  age,
  encryption,
  signalStrength,
  initialSnapshot,
} from "./types";

const CHANNELS: { label: string; value: number | null }[] = [
  { label: "Auto hop (2.4/5 GHz)", value: null },
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14].map((n) => ({
    label: `Channel ${n}`, value: n,
  })),
  ...[36, 40, 44, 48, 52, 56, 60, 64, 100, 104, 108, 112, 149, 153, 157, 161, 165].map(
    (n) => ({ label: `Channel ${n} (5 GHz)`, value: n }),
  ),
];

export function Scan({
  onBusy,
  pushLog,
  visible = true,
}: {
  visible?: boolean;
  onBusy: (v: boolean) => void;
  pushLog: (e: LogEvent) => void;
}) {
  const [ifaces, setIfaces] = useState<WirelessInterface[]>([]);
  const [iface, setIface] = useState("");
  const [channel, setChannel] = useState<number | null>(null);
  const [snapshot, setSnapshot] = useState<ScanSnapshot>(initialSnapshot);
  const [scanning, setScanning] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [os, setOs] = useState("");
  const [clock, setClock] = useState(Date.now());
  const [query, setQuery] = useState("");

  useEffect(() => {
    const t = setInterval(() => setClock(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  useEffect(() => {
    let active = true;
    platform()
      .then((p) => active && setOs(p))
      .catch(() => {});
    if (!visible) return;
    listInterfaces()
      .then((l) => {
        if (!active) return;
        setIfaces(l);
        setIface((current) => l.some((i) => i.name === current) ? current : (l.find((i) => i.interfaceType === "monitor") ?? l[0])?.name ?? "");
      })
      .catch((e) => setError(String(e)));
    return () => {
      active = false;
    };
  }, [visible]);

  const linux = os === "linux" || !native;
  useJobCompletion("scan", scanning && !pending, () => {
    setScanning(false);
    onBusy(false);
    setSnapshot((s) => ({ ...s, running: false, message: "Scan finished. Review the log for details." }));
  }, setError);

  async function start() {
    if (pending || scanning) return;
    if (!iface) {
      setError("Choose an interface first.");
      return;
    }
    setError("");
    setSnapshot({ ...initialSnapshot, interface: iface, running: true, message: "Starting scan…" });
    setPending(true);
    setScanning(true);
    onBusy(true);
    try {
      await startScan(
        iface,
        channel,
        (s) => {
          setSnapshot(s);
          setScanning(s.running);
          onBusy(s.running);
        },
        pushLog,
      );
    } catch (e) {
      setError(String(e));
      setScanning(false);
      onBusy(false);
    } finally {
      setPending(false);
    }
  }
  async function stop() {
    if (pending) return;
    setPending(true);
    try {
      await stopScan();
      setScanning(false);
      onBusy(false);
      setSnapshot((p) => ({ ...p, running: false, message: "Scan stopped" }));
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }

  const nets = snapshot.networks.filter((n) =>
    `${n.essid} ${n.bssid}`.toLowerCase().includes(query.toLowerCase()),
  );
  const clientsFor = (bssid: string) =>
    snapshot.clients.filter((c) => c.bssid === bssid).length;
  const seen = snapshot.networks.filter(
    (n) => clock - n.lastSeen < 15000,
  ).length;
  const running = scanning;

  return (
    <>
      {!linux && (
        <div className="notice warn">
          <Info size={15} />
          <span>
            Live scanning needs a Linux system with a compatible wireless
            chipset (Atheros, Ralink or Realtek). On {os.toUpperCase()} you can
            still crack existing captures in the Crack tab.
          </span>
        </div>
      )}
      <section className="toolbar">
        <div className="control">
          <label htmlFor="scan-iface">INTERFACE</label>
          <select
            id="scan-iface"
            disabled={scanning}
            value={iface}
            onChange={(e) => setIface(e.target.value)}
          >
            {ifaces.length ? (
              ifaces.map((i) => (
                <option key={i.name} value={i.name}>
                  {i.name}
                  {i.interfaceType === "monitor" ? " (monitor)" : ""}
                </option>
              ))
            ) : (
              <option value="">No interfaces</option>
            )}
          </select>
        </div>
        <div className="control">
          <label htmlFor="scan-channel">CHANNEL</label>
          <select
            id="scan-channel"
            disabled={scanning}
            value={channel === null ? "null" : String(channel)}
            onChange={(e) =>
              setChannel(e.target.value === "null" ? null : +e.target.value)
            }
          >
            {CHANNELS.map((c) => (
              <option
                key={c.value ?? "auto"}
                value={c.value === null ? "null" : String(c.value)}
              >
                {c.label}
              </option>
            ))}
          </select>
        </div>
        <button
          className="primary"
          disabled={pending || (!running && (!iface || !linux))}
          onClick={() => (running ? stop() : start())}
        >
          {running ? <Square size={16} /> : <Play size={16} />}{" "}
          {running ? "Stop scan" : "Start scan"}
        </button>
      </section>
      {error && (
        <div className="error" role="alert">
          <Info size={16} />
          <span>{error}</span>
          <button aria-label="Dismiss error" onClick={() => setError("")}>
            ✕
          </button>
        </div>
      )}
      <section className="stats">
        <div>
          <span>NETWORKS</span>
          <strong>
            {String(snapshot.networks.length).padStart(2, "0")}
            <small>{seen} seen in last 15s</small>
          </strong>
        </div>
        <div>
          <span>CLIENTS</span>
          <strong>
            {String(snapshot.clients.length).padStart(2, "0")}
            <small>Associated stations</small>
          </strong>
        </div>
        <div>
          <span>CHANNEL</span>
          <strong>
            {snapshot.channel || "—"}
            <em>chan</em>
          </strong>
        </div>
        <div>
          <span>INTERFACE</span>
          <strong>
            <small style={{ marginLeft: 0 }}>{snapshot.interface || "—"}</small>
          </strong>
        </div>
      </section>
      <div className="notice" role="status">{snapshot.message}</div>
      <div className="split-grid">
        <section className="panel">
          <div className="panel-heading">
            <h2>
              <Wifi size={16} />
              Networks
              <span className="count">{nets.length}</span>
            </h2>
            <div className="search">
              <Search size={13} />
              <input
                aria-label="Filter networks"
                placeholder="Search…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
          </div>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>NETWORK</th>
                  <th>CHANNEL</th>
                  <th>ENCRYPTION</th>
                  <th>POWER</th>
                  <th>CLIENTS</th>
                  <th>LAST SEEN</th>
                </tr>
              </thead>
              <tbody>
                {nets.map((n) => {
                  const enc = encryption(n.privacy);
                  const bars = Math.round(signalStrength(n.power) * 5);
                  return (
                    <tr key={n.bssid}>
                      <td>
                        <strong style={{ fontWeight: 500 }}>{n.essid}</strong>
                        <small className="mono" style={{ display: "block", color: "var(--muted)", marginTop: 3 }}>
                          {n.bssid}
                        </small>
                      </td>
                      <td className="mono">{n.channel}</td>
                      <td>
                        <span className={`badge ${enc.kind}`}>{enc.label}</span>
                        {n.wps && (
                          <span className="badge wpa" style={{ marginLeft: 4 }}>
                            WPS
                          </span>
                        )}
                      </td>
                      <td>
                        <div className="signal-bars">
                          {[1, 2, 3, 4, 5].map((i) => (
                            <i
                              key={i}
                              className={i <= bars ? "lit" : ""}
                              style={{ height: 4 + i * 2 }}
                            />
                          ))}
                          <span>{n.power} dBm</span>
                        </div>
                      </td>
                      <td className="mono">{clientsFor(n.bssid)}</td>
                      <td className="last-seen">{age(n.lastSeen, clock)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {!nets.length && (
              <div className="empty">
                <Wifi size={33} />
                <strong>
                  {running
                    ? "Listening for networks…"
                    : query
                      ? "No matching networks"
                      : "No networks detected yet."}
                </strong>
                <p>
                  {running
                    ? "Access points within range will appear here as they beacon."
                    : "Start a scan to discover networks, or explore the synthetic demo in the browser."}
                </p>
              </div>
            )}
          </div>
          <div className="panel-footer">
            <Info size={12} />
            ESSIDs and MACs are broadcast by access points in range. Only survey
            networks you are authorized to test.
          </div>
        </section>
        <section className="panel">
          <div className="panel-heading">
            <h2>
              <Radio size={16} />
              Clients
              <span className="count">{snapshot.clients.length}</span>
            </h2>
            <span className={`live-label ${running ? "on" : ""}`}>
              <i />
              {running ? "SURVEYING" : "STANDBY"}
            </span>
          </div>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>STATION MAC</th>
                  <th>POWER</th>
                  <th>PACKETS</th>
                  <th>BSSID</th>
                </tr>
              </thead>
              <tbody>
                {snapshot.clients.map((c) => (
                  <tr key={c.mac}>
                    <td>
                      <span className="mono">{c.mac}</span>
                      {c.probedEssids && (
                        <small
                          style={{ display: "block", color: "var(--muted)", marginTop: 3 }}
                        >
                          probes: {c.probedEssids}
                        </small>
                      )}
                    </td>
                    <td className="mono">{c.power} dBm</td>
                    <td className="mono">{c.packets}</td>
                    <td className="mono">{c.bssid}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!snapshot.clients.length && (
              <div className="empty">
                <Radio size={30} />
                <strong>No clients seen.</strong>
                <p>
                  Stations appear once they transmit. Probe requests and data
                  frames reveal client MACs.
                </p>
              </div>
            )}
          </div>
          <div className="panel-footer">
            <Info size={12} />
            Client MACs are observed, not identified. No device is named.
          </div>
        </section>
      </div>
    </>
  );
}
