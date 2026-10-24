import React, { useEffect, useRef, useState } from "react";
import { Wifi, Play, Square, Radio, Search, Info, ArrowRight } from "lucide-react";
import { listInterfaces, startScan, stopScan, platform, native } from "./bridge";
import { Preflight } from "./Preflight";
import { useJobCompletion } from "./useJobCompletion";
import {
  Network,
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
  onTargets,
  onInterfaces,
}: {
  visible?: boolean;
  onInterfaces?: () => void;
  onTargets?: (networks: Network[]) => void;
  onBusy: (v: boolean) => void;
  pushLog: (e: LogEvent) => void;
}) {
  const [ifaces, setIfaces] = useState<WirelessInterface[]>([]);
  const generation = useRef(0);
  const [iface, setIface] = useState("");
  const [channel, setChannel] = useState<number | null>(null);
  const [snapshot, setSnapshot] = useState<ScanSnapshot>(initialSnapshot);
  const [scanning, setScanning] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [os, setOs] = useState("");
  const [clock, setClock] = useState(Date.now());
  const [selection, setSelection] = useState<string[]>([]);
  const [multiple, setMultiple] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [endedAt, setEndedAt] = useState<number | null>(null);
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
    setEndedAt(Date.now());
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
    const session = ++generation.current;
    setSelection([]);
    setStartedAt(Date.now());
    setEndedAt(null);
    setSnapshot({ ...initialSnapshot, interface: iface, running: true, message: "Starting scan…" });
    setPending(true);
    setScanning(true);
    onBusy(true);
    try {
      await startScan(
        iface,
        channel,
        (s) => {
          if (session !== generation.current) return;
          setSnapshot(s);
          setScanning(s.running);
          if (!s.running) setEndedAt(Date.now());
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
    setStopping(true);
    try {
      await stopScan();
      generation.current++;
      setEndedAt(Date.now());
      setScanning(false);
      onBusy(false);
      setSnapshot((p) => ({ ...p, running: false, message: "Scan stopped" }));
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
      setStopping(false);
    }
  }

  const selectedNetworks = snapshot.networks.filter(n => selection.includes(n.bssid));
  const nets = snapshot.networks.filter((n) =>
    `${n.essid} ${n.bssid}`.toLowerCase().includes(query.toLowerCase()),
  );
  const clientsFor = (bssid: string) =>
    snapshot.clients.filter((c) => c.bssid === bssid).length;
  const seen = snapshot.networks.filter(
    (n) => clock - n.lastSeen < 15000,
  ).length;
  const running = scanning;

  const seconds = startedAt ? Math.max(0, Math.floor(((endedAt ?? clock) - startedAt) / 1000)) : 0;
  const duration = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map(n => String(n).padStart(2, "0")).join(":");
  const toggle = (bssid: string) => setSelection(p => multiple ? p.includes(bssid) ? p.filter(b => b !== bssid) : [...p, bssid] : [bssid]);
  return <>
    <section className="discovery-hero panel">
      <div><h1>Network Discovery</h1><p>Scan for nearby Wi-Fi networks and select targets for further analysis.</p></div>
      <div className="scan-actions">
        <div className="interface-picker"><Radio size={22} /><select aria-label="Scan interface" disabled={scanning || pending} value={iface} onChange={e => setIface(e.target.value)}>
          {ifaces.length ? ifaces.map(i => <option key={i.name} value={i.name}>{i.name}{i.interfaceType === "monitor" ? " (monitor)" : " (managed)"}</option>) : <option value="">No interfaces</option>}
        </select></div>
        <button className={running ? "scan-stop" : "primary"} disabled={pending || (!running && (!iface || !linux))} onClick={() => running ? stop() : start()}>
          {running ? <Square size={17} fill="currentColor" /> : <Play size={17} />} {stopping ? "Stopping…" : pending ? "Starting…" : running ? "Stop scan" : "Start scan"}
        </button>
        <div className="scan-duration"><span>Scan duration</span><strong>{duration}</strong></div>
      </div>
    </section>
    {error && <div className="error" role="alert"><Info size={16}/><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError("")}>Dismiss</button></div>}
    {!linux && <div className="notice warn">Live scanning requires Linux with a compatible wireless adapter.</div>}
    <div className="discovery-layout">
      <section className="panel networks-panel">
        <div className="panel-heading">
          <h2>Discovered Networks ({nets.length})</h2>
          <div className="network-filters"><div className="search"><Search size={16}/><input aria-label="Filter networks" placeholder="Filter networks…" value={query} onChange={e => setQuery(e.target.value)}/></div>
          <select aria-label="Scan channel" disabled={scanning || pending} value={channel ?? "all"} onChange={e => setChannel(e.target.value === "all" ? null : +e.target.value)}>{CHANNELS.map(c => <option key={c.value ?? "all"} value={c.value ?? "all"}>{c.value === null ? "All channels" : c.label}</option>)}</select></div>
        </div>
        <div className="table-scroll network-table"><table>
          <thead><tr><th><input aria-label="Select all visible networks" type="checkbox" disabled={!multiple || !nets.length} checked={nets.length > 0 && nets.every(n => selection.includes(n.bssid))} onChange={e => setSelection(p => e.target.checked ? [...new Set([...p, ...nets.map(n => n.bssid)])] : p.filter(b => !nets.some(n => n.bssid === b)))}/></th><th>SSID</th><th>BSSID (MAC)</th><th>Channel</th><th>Signal (dBm)</th><th>Encryption</th><th>Clients</th><th>Last seen</th></tr></thead>
          <tbody>{nets.map(n => <tr key={n.bssid} className={selection.includes(n.bssid) ? "selected-network" : ""}>
            <td><input aria-label={`Select ${n.essid || n.bssid}`} type={multiple ? "checkbox" : "radio"} name="network-selection" checked={selection.includes(n.bssid)} onChange={() => toggle(n.bssid)}/></td>
            <td className="network-name">{n.essid || "Hidden Network"}</td><td className="mono">{n.bssid}</td><td>{n.channel}</td><td className={n.power > -60 ? "signal-good" : "signal-fair"}>{n.power}</td><td>{encryption(n.privacy).label}{n.wps ? " · WPS" : ""}</td><td>{clientsFor(n.bssid)}</td><td className="last-seen">{age(n.lastSeen, clock)}</td>
          </tr>)}</tbody>
        </table></div>
        <div className="network-cards">{nets.map(n => <label className={`network-card ${selection.includes(n.bssid) ? "selected-network" : ""}`} key={n.bssid}>
          <input aria-label={`Select mobile ${n.essid || n.bssid}`} type={multiple ? "checkbox" : "radio"} name="mobile-network-selection" checked={selection.includes(n.bssid)} onChange={() => toggle(n.bssid)}/>
          <span><strong>{n.essid || "Hidden Network"}</strong><small>{n.bssid} · CH {n.channel}</small><small>{encryption(n.privacy).label} · {clientsFor(n.bssid)} clients</small></span><span className="signal-good">{n.power} dBm</span>
        </label>)}</div>
        {!nets.length && <div className="empty"><Wifi size={30}/><strong>{running ? "Listening for networks…" : query ? "No matching networks" : "No networks detected yet."}</strong><p>{running ? "Access points appear as they transmit." : "Start a scan to discover nearby networks."}</p></div>}
        <div className="selection-footer"><span>{selectedNetworks.length} of {snapshot.networks.length} networks selected</span>
          <select aria-label="Selection" value={multiple ? "multiple" : "single"} onChange={e => {setMultiple(e.target.value === "multiple"); setSelection(p => p.slice(0,1));}}><option value="single">Single network</option><option value="multiple">Multiple networks</option></select>
          <button className="secondary" disabled={!selection.length} onClick={() => setSelection([])}>Clear selection</button>
          <button className="primary" disabled={!selectedNetworks.length || scanning || pending} onClick={() => onTargets?.(selectedNetworks)}>Review selected targets <ArrowRight size={16}/></button>
        </div>
        <div className="scan-status" role="status">{snapshot.message}{scanning && selectedNetworks.length > 0 ? " · Stop the scan to review targets." : ""}</div>
      </section>
      <Preflight interfaceName={iface} onInterfaces={onInterfaces}/>
    </div>
    {snapshot.clients.length > 0 && <details className="client-disclosure"><summary>Observed clients ({snapshot.clients.length})</summary><div className="table-scroll"><table><thead><tr><th>Station MAC</th><th>Signal</th><th>Packets</th><th>BSSID</th></tr></thead><tbody>{snapshot.clients.map(c => <tr key={c.mac}><td>{c.mac}</td><td>{c.power} dBm</td><td>{c.packets}</td><td>{c.bssid}</td></tr>)}</tbody></table></div></details>}
  </>;
}
