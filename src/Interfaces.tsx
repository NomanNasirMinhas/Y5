import React, { useEffect, useState } from "react";
import { Network, Info, Radio, RefreshCw, Power, ShieldAlert } from "lucide-react";
import {
  listInterfaces,
  startMonitor,
  stopMonitor,
  checkKill,
  platform,
  native,
} from "./bridge";
import { WirelessInterface } from "./types";

export function Interfaces({ busy = false }: { busy?: boolean }) {
  const [ifaces, setIfaces] = useState<WirelessInterface[]>([]);
  const [selected, setSelected] = useState("");
  const [os, setOs] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [pending, setPending] = useState(false);

  async function refresh() {
    try {
      const l = await listInterfaces();
      setIfaces(l);
      if (!l.find((i) => i.name === selected)) {
        setSelected(l[0]?.name ?? "");
      }
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    platform()
      .then((p) => setOs(p))
      .catch(() => {});
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const linux = os === "linux" || !native;

  async function start() {
    if (pending || busy) return;
    setPending(true);
    setError("");
    setResult("");
    try {
      setResult(`Monitor mode started: ${await startMonitor(selected)}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }
  async function stop() {
    if (pending || busy) return;
    setPending(true);
    setError("");
    setResult("");
    try {
      setResult(await stopMonitor(selected));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }
  async function kill() {
    if (pending || busy) return;
    if (!window.confirm("Stop conflicting network services? This may disconnect this computer from Wi-Fi.")) return;
    setPending(true);
    setError("");
    setResult("");
    try {
      setResult(await checkKill());
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }

  return (
    <>
      {!native && <div className="notice">Demo interfaces are synthetic. Monitor controls require the Linux desktop app.</div>}
      {busy && <div className="notice">Stop active jobs before changing radio settings.</div>}
      {!linux && (
        <div className="notice warn">
          <ShieldAlert size={15} />
          <span>
            Monitor mode requires Linux with a compatible chipset. On{" "}
            {os.toUpperCase()} you can list interfaces but cannot switch to
            monitor mode.
          </span>
        </div>
      )}
      <section className="panel">
        <div className="panel-heading">
          <h2>
            <Network size={16} />
            Wireless interfaces
            <span className="count">{ifaces.length}</span>
          </h2>
          <button className="secondary" onClick={refresh}>
            <RefreshCw size={14} />
            Refresh
          </button>
        </div>
        {ifaces.length ? (
          ifaces.map((i) => (
            <div
              key={i.name}
              className={`interface-card${selected === i.name ? " selected" : ""}`}
              role="button"
              tabIndex={pending ? -1 : 0}
              aria-pressed={selected === i.name}
              onClick={() => !pending && setSelected(i.name)}
              onKeyDown={(e) => {
                if (!pending && (e.key === "Enter" || e.key === " ")) {
                  e.preventDefault(); setSelected(i.name);
                }
              }}
            >
              <div className="interface-icon">
                {i.interfaceType === "monitor" ? (
                  <Radio size={18} />
                ) : (
                  <Network size={18} />
                )}
              </div>
              <div>
                <strong>
                  {i.name}
                  {i.interfaceType === "monitor" && (
                    <span className="badge wpa3" style={{ marginLeft: 8 }}>
                      MONITOR
                    </span>
                  )}
                </strong>
                <small>
                  {i.mac || "unknown MAC"} · {i.interfaceType} · {i.state}
                </small>
              </div>
            </div>
          ))
        ) : (
          <div className="empty">
            <Network size={33} />
            <strong>No wireless interfaces found.</strong>
            <p>
              Plug in a compatible adapter and refresh. On Linux, ensure{" "}
              <span className="mono">iw</span> and{" "}
              <span className="mono">aircrack-ng</span> are installed.
            </p>
          </div>
        )}
        <div className="panel-footer">
          <Info size={12} />
          Monitor interfaces capture all frames on a channel; managed
          interfaces only see their own traffic.
        </div>
      </section>
      <section className="toolbar" style={{ marginTop: 18 }}>
        <div className="control">
          <label htmlFor="mon-iface">INTERFACE</label>
          <select
            id="mon-iface"
            disabled={pending || busy}
            value={selected}
            onChange={(e) => setSelected(e.target.value)}
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
        <button className="primary" disabled={pending || busy || !native || !linux || !selected || ifaces.find((i) => i.name === selected)?.interfaceType === "monitor"} onClick={start}>
          <Power size={15} />
          Start monitor
        </button>
        <button className="secondary" disabled={pending || busy || !native || !linux || !selected || ifaces.find((i) => i.name === selected)?.interfaceType !== "monitor"} onClick={stop}>
          <Power size={15} />
          Stop monitor
        </button>
        <button className="danger" disabled={pending || busy || !native || !linux} onClick={kill}>
          <ShieldAlert size={15} />
          Check kill
        </button>
      </section>
      {result && (
        <div className="notice" style={{ marginTop: 14 }}>
          <Info size={14} />
          <span style={{ whiteSpace: "pre-wrap" }}>{result}</span>
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
    </>
  );
}
