import React, { useEffect, useState } from "react";
import { useJobCompletion } from "./useJobCompletion";
import { validMac } from "./validation";
import {
  KeyRound,
  Info,
  Play,
  Square,
  FolderOpen,
  FileKey,
  Download,
  ShieldAlert,
  Cpu,
} from "lucide-react";
import {
  listCaptures,
  listWordlists,
  listRules,
  startCrack,
  startHashcat,
  stopCrack,
  exportHashcat,
  revealCaptures,
  hashcatInfo,
  native,
} from "./bridge";
import {
  CaptureInfo,
  Wordlist,
  CrackStatus,
  HashcatOptions,
  LogEvent,
  fmtBytes,
} from "./types";

type Engine = "aircrack" | "hashcat";
type Attack = "wordlist" | "rules" | "mask";

export function Crack({
  onBusy,
  pushLog,
  active,
}: {
  onBusy: (v: boolean) => void;
  pushLog: (e: LogEvent) => void;
  active: boolean;
}) {
  const [engine, setEngine] = useState<Engine>("aircrack");
  const [captures, setCaptures] = useState<CaptureInfo[]>([]);
  const [wordlists, setWordlists] = useState<Wordlist[]>([]);
  const [rules, setRules] = useState<Wordlist[]>([]);
  const [capture, setCapture] = useState("");
  const [wordlist, setWordlist] = useState("");
  const [bssid, setBssid] = useState("");
  const [mode, setMode] = useState("auto");
  const [hashMode, setHashMode] = useState("auto");
  const [attack, setAttack] = useState<Attack>("wordlist");
  const [rule, setRule] = useState("");
  const [mask, setMask] = useState("");
  const [increment, setIncrement] = useState(true);
  const [status, setStatus] = useState<CrackStatus | null>(null);
  const [running, setRunning] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [exportMsg, setExportMsg] = useState("");
  const [deviceInfo, setDeviceInfo] = useState("");
  useJobCompletion("crack", running && !pending, () => {
    setRunning(false);
    onBusy(false);
    setStatus((s) => s?.running ? { ...s, running: false, message: "Worker exited. Review the log for details." } : s);
  }, setError);

  async function refresh() {
    if (!native) return;
    try {
      setCaptures(await listCaptures());
      setWordlists(await listWordlists());
      setRules(await listRules());
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  async function start() {
    if (pending || running) return;
    setError("");
    setExportMsg("");
    if (!capture) {
      setError("Choose a capture first.");
      return;
    }
    if (!canStart) {
      setError("Select the required wordlist, rule or mask before starting.");
      return;
    }
    if (bssid.trim() && !validMac(bssid)) {
      setError("Enter a valid BSSID or leave it empty for auto-detection.");
      return;
    }
    setStatus(null);
    setPending(true);
    setRunning(true);
    onBusy(true);
    const receiveStatus = (s: CrackStatus) => {
      setStatus(s);
      setRunning(s.running);
      onBusy(s.running);
    };
    try {
      if (engine === "aircrack") {
        if (!wordlist) {
          setError("Choose a wordlist.");
          return;
        }
        await startCrack(
          capture,
          wordlist,
          bssid.trim() || null,
          mode,
          receiveStatus,
          pushLog,
        );
      } else {
        const opts: HashcatOptions = {
          capture,
          hashMode: hashMode === "auto" ? null : +hashMode,
          attack,
          wordlist: attack === "mask" ? null : wordlist || null,
          rules: attack === "rules" ? rule || null : null,
          mask: attack === "mask" ? mask : null,
          increment,
        };
        await startHashcat(opts, receiveStatus, pushLog);
      }
    } catch (e) {
      setError(String(e));
      setRunning(false);
      onBusy(false);
    } finally {
      setPending(false);
    }
  }
  async function stop() {
    if (pending) return;
    setPending(true);
    try {
      await stopCrack();
      setRunning(false);
      onBusy(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }
  async function exportOne() {
    setError("");
    setExportMsg("");
    if (!capture) {
      setError("Choose a capture first.");
      return;
    }
    try {
      setExportMsg(await exportHashcat(capture));
    } catch (e) {
      setError(String(e));
    }
  }
  async function showDevices() {
    setError("");
    try {
      setDeviceInfo(await hashcatInfo());
    } catch (e) {
      setError(String(e));
      setDeviceInfo("");
    }
  }

  const picked = captures.find((c) => c.path === capture);
  const needsWordlist = engine === "aircrack" || attack !== "mask";
  const canStart =
    !!capture &&
    (engine === "hashcat"
      ? attack === "mask"
        ? !!mask.trim()
        : !!wordlist && (attack !== "rules" || !!rule)
      : !!wordlist);

  return (
    <>
      <div className="notice warn">
        <ShieldAlert size={15} />
        <span>
          <b>Authorized targets only.</b> Crack only captures you own or are
          permitted to test. Both engines run entirely on this machine.
        </span>
      </div>
      <div className="engine-toggle" role="radiogroup" aria-label="Cracking engine">
        <button
          className={`engine-option${engine === "aircrack" ? " active" : ""}`}
          role="radio"
          aria-checked={engine === "aircrack"}
          disabled={running}
          onClick={() => setEngine("aircrack")}
        >
          Aircrack-ng
        </button>
        <button
          className={`engine-option${engine === "hashcat" ? " active" : ""}`}
          role="radio"
          aria-checked={engine === "hashcat"}
          disabled={running}
          onClick={() => setEngine("hashcat")}
        >
          Hashcat
        </button>
      </div>
      <div className="split-grid">
        <section className="panel form-panel">
          <div className="panel-heading" style={{ border: 0, padding: 0, height: "auto" }}>
            <h2>
              <KeyRound size={16} />
              {engine === "aircrack" ? "Crack a capture" : "Hashcat attack"}
            </h2>
            <button className="secondary" disabled={!native} onClick={() => revealCaptures().catch((e) => setError(String(e)))}>
              <FolderOpen size={14} />
              Open folder
            </button>
          </div>
          <div className="field">
            <label htmlFor="crack-capture">CAPTURE</label>
            <select
              id="crack-capture"
              disabled={running}
              value={capture}
              onChange={(e) => setCapture(e.target.value)}
            >
              <option value="">Select a capture…</option>
              {captures.map((c) => (
                <option key={c.path} value={c.path}>
                  {c.name} · {fmtBytes(c.bytes)}
                </option>
              ))}
            </select>
          </div>
          {engine === "aircrack" ? (
            <>
              <div className="field">
                <label htmlFor="crack-wordlist">WORDLIST</label>
                <select
                  id="crack-wordlist"
                  disabled={running}
                  value={wordlist}
                  onChange={(e) => setWordlist(e.target.value)}
                >
                  <option value="">Select a wordlist…</option>
                  {wordlists.map((w) => (
                    <option key={w.path} value={w.path}>
                      {w.name} · {fmtBytes(w.bytes)}
                    </option>
                  ))}
                </select>
              </div>
              <div className="form-row">
                <div className="field">
                  <label htmlFor="crack-bssid">BSSID (optional)</label>
                  <input
                    id="crack-bssid"
                    placeholder="Auto-detect"
                    value={bssid}
                    disabled={running}
                    onChange={(e) => setBssid(e.target.value)}
                  />
                </div>
                <div className="field">
                  <label htmlFor="crack-mode">MODE</label>
                  <select
                    id="crack-mode"
                    disabled={running}
                    value={mode}
                    onChange={(e) => setMode(e.target.value)}
                  >
                    <option value="auto">Auto (WPA/WEP)</option>
                    <option value="wpa">WPA/WPA2</option>
                    <option value="wep">WEP</option>
                  </select>
                </div>
              </div>
            </>
          ) : (
            <>
              <div className="form-row">
                <div className="field">
                  <label htmlFor="hc-mode">HASH MODE</label>
                  <select
                    id="hc-mode"
                    disabled={running}
                    value={hashMode}
                    onChange={(e) => setHashMode(e.target.value)}
                  >
                    <option value="auto">Auto (22000 / 2500)</option>
                    <option value="22000">22000 · WPA-PMKID-EAPOL</option>
                    <option value="2500">2500 · WPA legacy hccap</option>
                  </select>
                </div>
                <div className="field">
                  <label htmlFor="hc-attack">ATTACK MODE</label>
                  <select
                    id="hc-attack"
                    disabled={running}
                    value={attack}
                    onChange={(e) => setAttack(e.target.value as Attack)}
                  >
                    <option value="wordlist">Wordlist (straight)</option>
                    <option value="rules">Wordlist + rules</option>
                    <option value="mask">Mask (brute force)</option>
                  </select>
                </div>
              </div>
              {attack !== "mask" && (
                <div className="field">
                  <label htmlFor="hc-wordlist">WORDLIST</label>
                  <select
                    id="hc-wordlist"
                    disabled={running}
                    value={wordlist}
                    onChange={(e) => setWordlist(e.target.value)}
                  >
                    <option value="">Select a wordlist…</option>
                    {wordlists.map((w) => (
                      <option key={w.path} value={w.path}>
                        {w.name} · {fmtBytes(w.bytes)}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              {attack === "rules" && (
                <div className="field">
                  <label htmlFor="hc-rules">RULES</label>
                  <select
                    id="hc-rules"
                    disabled={running}
                    value={rule}
                    onChange={(e) => setRule(e.target.value)}
                  >
                    <option value="">Select rules…</option>
                    {rules.map((r) => (
                      <option key={r.path} value={r.path}>
                        {r.name}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              {attack === "mask" && (
                <div className="form-row">
                  <div className="field">
                    <label htmlFor="hc-mask">MASK</label>
                    <input
                      id="hc-mask"
                      placeholder="?d?d?d?d?d?d?d?d"
                      value={mask}
                      disabled={running}
                      onChange={(e) => setMask(e.target.value)}
                    />
                  </div>
                  <div className="field" style={{ minWidth: 120 }}>
                    <label htmlFor="hc-increment">INCREMENT</label>
                    <select
                      id="hc-increment"
                      disabled={running}
                      value={increment ? "on" : "off"}
                      onChange={(e) => setIncrement(e.target.value === "on")}
                    >
                      <option value="on">Grow mask from length 1</option>
                      <option value="off">Fixed length</option>
                    </select>
                  </div>
                </div>
              )}
            </>
          )}
          <div className="attack-actions">
            <button
              className={running ? "danger" : "primary"}
              disabled={pending || !native || (!running && !canStart)}
              onClick={running ? stop : start}
            >
              {running ? <Square size={15} /> : <Play size={15} />}
              {running
                ? "Stop"
                : `Start ${engine === "aircrack" ? "aircrack-ng" : "hashcat"}`}
            </button>
            {engine === "aircrack" && (
              <button className="secondary" disabled={running || pending || !native || !capture} onClick={exportOne}>
                <Download size={15} />
                Export hashcat
              </button>
            )}
            {engine === "hashcat" && (
              <button className="secondary" disabled={running || pending || !native} onClick={showDevices}>
                <Cpu size={15} />
                Device info
              </button>
            )}
          </div>
          {picked && (
            <div className="notice">
              <FileKey size={14} />
              <span>
                {picked.name} · captured{" "}
                {new Date(picked.created).toLocaleString()}
              </span>
            </div>
          )}
          {exportMsg && (
            <div className="notice">
              <Info size={14} />
              <span>{exportMsg}</span>
            </div>
          )}
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
        <section className="panel form-panel">
          <div className="panel-heading" style={{ border: 0, padding: 0, height: "auto" }}>
            <h2>
              <KeyRound size={16} />
              Progress
            </h2>
            <span className={`live-label ${running ? "on" : ""}`}>
              <i />
              {running ? "CRACKING" : "IDLE"}
            </span>
          </div>
          {status?.key ? (
            <div className="key-found">
              <span>KEY RECOVERED</span>
              <strong>{status.key}</strong>
              <p style={{ marginTop: 6, fontSize: 11 }}>
                {status.mode} · {status.message}
              </p>
            </div>
          ) : (
            <div className="notice">
              <Info size={14} />
              <span>
                {status?.message || (running
                  ? "Running…"
                  : engine === "hashcat"
                    ? "Pick a capture and attack mode to run hashcat here — GPU-accelerated, no external terminal needed."
                    : "Choose a capture and wordlist to begin. Recovery speed depends on the wordlist size and your CPU.")}
              </span>
            </div>
          )}
          {engine === "hashcat" && (
            <div className="readouts">
              <div className="readout">
                <small>SPEED</small>
                <strong>{running ? status?.speed || "—" : "—"}</strong>
              </div>
              <div className="readout">
                <small>RECOVERED</small>
                <strong>{running ? status?.recovered || "—" : "—"}</strong>
              </div>
            </div>
          )}
          {running && (
            <div className="crack-progress">
              <div
                style={{
                  width: `${Math.round((status?.progress ?? 0) * 100)}%`,
                }}
              />
            </div>
          )}
          {running && status?.progress != null && (
            <div style={{ fontSize: 11, color: "var(--muted)" }}>
              {Math.round(status.progress * 100)}% of keyspace
            </div>
          )}
          {deviceInfo && engine === "hashcat" && (
            <pre className="device-pre">{deviceInfo}</pre>
          )}
          {!native && (
            <div className="notice warn">
              <ShieldAlert size={14} />
              <span>
                Cracking runs in the desktop app. The browser preview lists no
                captures or wordlists.
              </span>
            </div>
          )}
          {captures.length === 0 && native && (
            <div className="notice">
              <Info size={14} />
              <span>
                No crackable captures yet. Run a scan or handshake capture first
                — `.cap` / `.pcap` / `.22000` files appear here automatically.
              </span>
            </div>
          )}
          {wordlists.length === 0 && native && needsWordlist && (
            <div className="notice">
              <Info size={14} />
              <span>
                No wordlists found. Place `.txt`/`.lst`/`.gz` files in the app's
                wordlists folder, or install a system list like{" "}
                <span className="mono">rockyou.txt</span>.
              </span>
            </div>
          )}
          {engine === "hashcat" && attack === "rules" && rules.length === 0 && native && (
            <div className="notice">
              <Info size={14} />
              <span>
                No rules found. Install hashcat's rules (e.g.{" "}
                <span className="mono">/usr/share/hashcat/rules</span>) or place
                `.rule` files in the app's rules folder.
              </span>
            </div>
          )}
        </section>
      </div>
    </>
  );
}
