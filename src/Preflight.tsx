import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, AlertCircle, Info, Wifi, Settings, ChevronRight, RefreshCw } from "lucide-react";
import { invoke, native } from "./bridge";

type Check = { name: string; ok: boolean; required: boolean; detail: string };
export function Preflight({ interfaceName = "", onInterfaces }: { interfaceName?: string; onInterfaces?: () => void }) {
  const [restore, setRestore] = useState(true);
  const [checks, setChecks] = useState<Check[]>([]);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  async function refresh() {
    setPending(true);
    try { setChecks(native ? await invoke<Check[]>("preflight") : []); }
    catch (e) { setMessage(String(e)); }
    finally { setPending(false); }
  }
  useEffect(() => {
    void refresh();
    if (!native) return;
    const unlisten = listen<string>("recovery-error", e => setMessage(`Could not restore radios; app remains open. ${e.payload}`));
    return () => { void unlisten.then(fn => fn()); };
  }, []);
  async function recover() {
    setPending(true);
    try { setMessage(await invoke<string>("recover_interfaces")); }
    catch (e) { setMessage(String(e)); }
    finally { setPending(false); void refresh(); }
  }
  const missing = checks.filter(c => !c.ok && c.required).length;
  async function setRecovery(value: boolean) {
    setPending(true);
    try { if (native) await invoke("set_restore_on_exit", { enabled: value }); setRestore(value); }
    catch(e) { setMessage(String(e)); }
    finally { setPending(false); }
  }
  const previewChecks: Check[] = ["airmon-ng", "airodump-ng", "aireplay-ng", "iw", "aircrack-ng"].map(name => ({name, ok: false, required: true, detail: "Desktop check required"}));
  const shown = native ? checks.filter(c => c.required).slice(0,6) : previewChecks;
  return <aside className="preflight panel">
    <div className="tools-title"><h2>Startup Tools</h2><Info size={17}/></div>
    <div className="tool-list">{shown.map(c => <div className="tool-row" key={c.name} title={c.detail}>
      {c.ok ? <CheckCircle2 size={20} className="check-ok"/> : <AlertCircle size={20} className="check-warn"/>}<span>{c.name}</span><small>{!native ? "Preview" : c.ok ? "Ready" : "Missing"}</small>
    </div>)}</div>
    <div className="tools-summary"><CheckCircle2 size={23}/><span>{!native ? "Hardware checks in desktop app" : pending ? "Checking tools…" : missing ? `${missing} checks need attention` : checks.length ? "All core checks passed" : "Checks unavailable"}</span></div>
    <button className="radio-link" onClick={onInterfaces}><Wifi size={21}/><span>{interfaceName || "Choose interface"}</span><ChevronRight size={17}/></button>
    <label className="restore-option"><Settings size={21}/><input type="checkbox" disabled={pending} checked={restore} onChange={e => void setRecovery(e.target.checked)}/><span>Restore on exit<small>Return session interfaces to managed mode when the app exits.</small></span></label>
    <details className="tool-details"><summary>Diagnostics & recovery</summary>
      {checks.map(c => <p key={c.name}><strong>{c.name}</strong><small>{c.detail}</small></p>)}
      <button disabled={pending} onClick={refresh}><RefreshCw size={14}/> Recheck tools</button>
      <button disabled={pending || !native} onClick={recover}>Stop jobs & restore radios</button>
    </details>
    {message && <div className="recovery-message" role="status">{message}</div>}
  </aside>;
}
