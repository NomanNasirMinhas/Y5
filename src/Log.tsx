import React, { useEffect, useRef, useState } from "react";
import { Terminal, Trash2 } from "lucide-react";
import { LogEvent } from "./types";

export function Log({ logs, onClear, compact = false }: { logs: LogEvent[]; onClear: () => void; compact?: boolean }) {
  const [autoScroll, setAutoScroll] = useState(true);
  const [level, setLevel] = useState("all");
  const consoleRef = useRef<HTMLDivElement>(null);
  const shown = logs.filter(e => level === "all" || e.level === level);
  useEffect(() => { if (autoScroll && consoleRef.current) consoleRef.current.scrollTop = consoleRef.current.scrollHeight; }, [logs, autoScroll, level]);
  return <section className={`panel activity-panel ${compact ? "compact" : ""}`}>
    <div className="panel-heading"><h2><Terminal size={22}/> Activity Console</h2>
      <div className="console-controls"><button className="secondary" onClick={onClear} disabled={!logs.length}><Trash2 size={15}/> Clear</button>
        <label><input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)}/> Auto-scroll</label>
        <select aria-label="Log level" value={level} onChange={e => setLevel(e.target.value)}><option value="all">All levels</option><option value="info">Info</option><option value="warn">Warnings</option><option value="error">Errors</option></select>
      </div>
    </div>
    <div className="log-console" ref={consoleRef}>{shown.length ? shown.map((e,i) => <div key={i} className={`log-line ${e.level}`}><span className="log-time">[{new Date(e.timestamp).toLocaleTimeString([], {hour12:false})}]</span><span>{e.message}</span></div>) : <div className="log-line muted">Session output will appear here as tools run.</div>}</div>
  </section>;
}
