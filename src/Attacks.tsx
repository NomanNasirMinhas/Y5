import React, { useEffect, useState } from "react";
import { Crosshair, Info, WifiOff, Handshake, ShieldAlert, Square } from "lucide-react";
import {
  listInterfaces,
  deauth,
  startDeauth,
  stopAttack,
  captureHandshake,
  capturePmkid,
  wpsAttack,
  native,
  platform,
} from "./bridge";
import { WirelessInterface, LogEvent, Network } from "./types";
import { validateTarget } from "./validation";
import { useJobCompletion } from "./useJobCompletion";

type AttackKind = "deauth" | "handshake" | "pmkid" | "wps" | null;

const fmtTime = (t: number) =>
  new Date(t).toLocaleTimeString([], { hour12: false });

export function Attacks({
  onBusy,
  pushLog,
  visible = true,
  targets = [],
}: {
  visible?: boolean;
  targets?: Network[];
  onBusy: (v: boolean) => void;
  pushLog: (e: LogEvent) => void;
}) {
  const [ifaces, setIfaces] = useState<WirelessInterface[]>([]);
  const [iface, setIface] = useState("");
  const [bssid, setBssid] = useState("");
  const [client, setClient] = useState("");
  const [channel, setChannel] = useState("6");
  const [count, setCount] = useState("10");
  const [active, setActive] = useState<AttackKind>(null);
  const [bursting, setBursting] = useState(false);
  const [pending, setPending] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState("");
  const [log, setLog] = useState<LogEvent[]>([]);
  const [os, setOs] = useState("");

  useEffect(() => {
    let activeMount = true;
    platform()
      .then((p) => activeMount && setOs(p))
      .catch(() => {});
    if (!visible) return;
    listInterfaces()
      .then((l) => {
        if (!activeMount) return;
        setIfaces(l);
        const mon = l.find((i) => i.interfaceType === "monitor") ?? l[0];
        setIface((current) => l.some((i) => i.name === current) ? current : mon?.name ?? "");
      })
      .catch((e) => setError(String(e)));
    return () => {
      activeMount = false;
    };
  }, [visible]);

  useEffect(() => {
    if (!active && !bursting && !pending && targets.length) {
      setBssid(targets[0].bssid); setChannel(String(targets[0].channel)); setClient("");
    }
  }, [targets]);

  const linux = os === "linux" || !native;
  useJobCompletion("attack", active !== null && !pending, () => {
    setActive(null);
    void listInterfaces().then(setIfaces).catch(e => setError(String(e)));
    onBusy(false);
    onLog({ level: "info", message: "Worker finished. Review the output for the result.", timestamp: Date.now() });
  }, setError);
  const onLog = (e: LogEvent) => {
    if (e.message.endsWith(": monitor mode ready") || e.message.endsWith(": managed mode restored")) {
      void listInterfaces().then(setIfaces).catch(error => setError(String(error)));
    }
    setLog((p) => [...p, e].slice(-300));
    pushLog(e);
  };

  async function burst() {
    const issue = validateTarget(bssid, client, channel, count);
    if (issue) { setError(issue); return; }
    if (pending || bursting || active) return;
    setError("");
    setBursting(true);
    onBusy(true);
    try {
      await deauth(
        iface,
        bssid.trim(),
        client.trim() || null,
        Math.max(1, +count || 1),
        onLog,
        +channel,
      );
      setActive("deauth");
    } catch (e) {
      setError(String(e));
      onBusy(false);
    } finally {
      setBursting(false);
    }
  }

  async function launch(kind: Exclude<AttackKind, null>) {
    const issue = validateTarget(bssid, client, channel, count);
    if (issue) { setError(issue); return; }
    if (pending || bursting || active) return;
    setError("");
    setPending(true);
    onBusy(true);
    try {
      if (kind === "deauth") {
        await startDeauth(iface, bssid.trim(), client.trim() || null, onLog, +channel);
      } else if (kind === "handshake") {
        await captureHandshake(iface, bssid.trim(), +channel || 1, onLog);
      } else if (kind === "pmkid") {
        await capturePmkid(iface, bssid.trim(), +channel || 1, onLog);
      } else {
        await wpsAttack(iface, bssid.trim(), +channel || 1, onLog);
      }
      setActive(kind);
      onBusy(true);
    } catch (e) {
      setError(String(e));
      onBusy(false);
    } finally {
      setPending(false);
    }
  }

  async function halt() {
    if (pending) return;
    setPending(true);
    setStopping(true);
    try {
      await stopAttack();
      void listInterfaces().then(setIfaces).catch(e => setError(String(e)));
      setActive(null);
      onBusy(false);
      onLog({ level: "warn", message: "Attack stopped.", timestamp: Date.now() });
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
      setStopping(false);
    }
  }

  const disabled = pending || !native || !linux || !iface || !bssid.trim();
  const running = active !== null;

  return (
    <>
      {(pending || bursting) && <div className="notice" role="status">{stopping ? "Stopping tools and restoring managed mode…" : "Preparing the selected interface…"}</div>}
      {!native && <div className="notice">Browser preview: live operations require the desktop app on Linux.</div>}
      {!linux && (
        <div className="notice warn">
          <ShieldAlert size={15} />
          <span>
            Packet injection and monitor mode require Linux with a compatible
            chipset. On {os.toUpperCase()} these attacks are unavailable.
          </span>
        </div>
      )}
      <div className="notice warn">
        <ShieldAlert size={15} />
        <span>
          <b>Authorized targets only.</b> Deauthentication and cracking disrupt
          wireless networks. Run these tools only against networks you own or
          are explicitly permitted to test.
        </span>
      </div>
      {targets.length > 0 && <section className="toolbar">
        <label htmlFor="review-target">Selected networks ({targets.length})</label>
        <select id="review-target" value={targets.some(t => t.bssid === bssid) ? bssid : ""} disabled={running || bursting || pending} onChange={e => {
          const target = targets.find(t => t.bssid === e.target.value);
          if (target) { setBssid(target.bssid); setChannel(String(target.channel)); setClient(""); }
        }}><option value="" disabled>Choose target</option>{targets.map(t => <option key={t.bssid} value={t.bssid}>{t.essid} · {t.bssid} · CH {t.channel}</option>)}</select>
        <small>Review and run one target at a time on this radio.</small>
      </section>}
      <div className="split-grid">
        <section className="panel form-panel">
          <div className="panel-heading" style={{ border: 0, padding: 0, height: "auto" }}>
            <h2>
              <Crosshair size={16} />
              Target
            </h2>
          </div>
          <div className="form-row">
            <div className="field">
              <label htmlFor="atk-iface">INTERFACE</label>
              <select
                id="atk-iface"
                disabled={running || bursting}
                value={iface}
                onChange={(e) => setIface(e.target.value)}
              >
                {ifaces.length ? (
                  ifaces.map((i) => (
                    <option key={i.name} value={i.name}>
                      {i.name}
                    </option>
                  ))
                ) : (
                  <option value="">No interfaces</option>
                )}
              </select>
            </div>
            <div className="field">
              <label htmlFor="atk-bssid">TARGET BSSID</label>
              <input
                id="atk-bssid"
                placeholder="AA:BB:CC:DD:EE:FF"
                value={bssid}
                disabled={running || bursting}
                onChange={(e) => setBssid(e.target.value)}
              />
            </div>
            <div className="field">
              <label htmlFor="atk-client">CLIENT MAC (optional)</label>
              <input
                id="atk-client"
                placeholder="Broadcast"
                value={client}
                disabled={running || bursting}
                onChange={(e) => setClient(e.target.value)}
              />
            </div>
          </div>
          <div className="form-row">
            <div className="field">
              <label htmlFor="atk-channel">CHANNEL</label>
              <input
                id="atk-channel"
                type="number"
                min="1"
                max="165"
                value={channel}
                disabled={running || bursting}
                onChange={(e) => setChannel(e.target.value)}
              />
            </div>
            <div className="field">
              <label htmlFor="atk-count">DEAUTH PACKETS</label>
              <input
                id="atk-count"
                type="number"
                min="1"
                max="9999"
                value={count}
                disabled={running || bursting}
                onChange={(e) => setCount(e.target.value)}
              />
            </div>
          </div>
          <div className="attack-actions">
            <button
              className="secondary"
              disabled={disabled || bursting || running}
              onClick={burst}
            >
              <WifiOff size={15} />
              Deauth burst
            </button>
            <button
              className={active === "deauth" ? "danger" : "secondary"}
              disabled={disabled || bursting || (running && active !== "deauth")}
              onClick={() => (active === "deauth" ? halt() : launch("deauth"))}
            >
              {active === "deauth" ? <Square size={15} /> : <WifiOff size={15} />}
              {active === "deauth" ? "Stop deauth" : "Continuous deauth"}
            </button>
          </div>
          <div className="attack-actions">
            <button
              className={active === "handshake" ? "danger" : "secondary"}
              disabled={disabled || bursting || (running && active !== "handshake")}
              onClick={() =>
                active === "handshake" ? halt() : launch("handshake")
              }
            >
              {active === "handshake" ? <Square size={15} /> : <Handshake size={15} />}
              {active === "handshake" ? "Stop capture" : "Capture handshake"}
            </button>
            <button
              className={active === "pmkid" ? "danger" : "secondary"}
              disabled={disabled || bursting || (running && active !== "pmkid")}
              onClick={() => (active === "pmkid" ? halt() : launch("pmkid"))}
            >
              {active === "pmkid" ? <Square size={15} /> : <ShieldAlert size={15} />}
              {active === "pmkid" ? "Stop capture" : "Capture PMKID"}
            </button>
            <button
              className={active === "wps" ? "danger" : "secondary"}
              disabled={disabled || bursting || (running && active !== "wps")}
              onClick={() => (active === "wps" ? halt() : launch("wps"))}
            >
              {active === "wps" ? <Square size={15} /> : <Crosshair size={15} />}
              {active === "wps" ? "Stop WPS" : "WPS attack"}
            </button>
          </div>
          {error && (
            <div className="error" role="alert">
              <Info size={16} />
              <span>{error}</span>
              <button aria-label="Dismiss error" onClick={() => setError("")}>
                ✕
              </button>
            </div>
          )}
        </section>
        <section className="panel">
          <div className="panel-heading">
            <h2>
              <Crosshair size={16} />
              Attack log
            </h2>
            <span className={`live-label ${running ? "on" : ""}`}>
              <i />
              {running ? "RUNNING" : "IDLE"}
            </span>
          </div>
          <div className="log-console" style={{ height: 460 }}>
            {log.length ? (
              log.map((e, i) => (
                <div key={i} className={`log-line ${e.level}`}>
                  <span className="log-time">{fmtTime(e.timestamp)}</span>
                  <span>{e.message}</span>
                </div>
              ))
            ) : (
              <div className="log-line" style={{ color: "var(--muted)" }}>
                Attack output appears here as tools run.
              </div>
            )}
          </div>
          <div className="panel-footer">
            <Info size={12} />
            aireplay-ng · hcxdumptool · reaver/bully (when installed)
          </div>
        </section>
      </div>
    </>
  );
}
