import React from "react";
import { ScrollText, Trash2 } from "lucide-react";
import { LogEvent } from "./types";

const fmtTime = (t: number) =>
  new Date(t).toLocaleTimeString([], { hour12: false });

export function Log({
  logs,
  onClear,
}: {
  logs: LogEvent[];
  onClear: () => void;
}) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <h2>
          <ScrollText size={16} />
          Unified event stream
          <span className="count">{logs.length}</span>
        </h2>
        <button className="secondary" onClick={onClear} disabled={!logs.length}>
          <Trash2 size={14} />
          Clear
        </button>
      </div>
      <div className="log-console">
        {logs.length ? (
          logs.map((e, i) => (
            <div key={i} className={`log-line ${e.level}`}>
              <span className="log-time">{fmtTime(e.timestamp)}</span>
              <span>{e.message}</span>
            </div>
          ))
        ) : (
          <div className="log-line" style={{ color: "var(--muted)" }}>
            Scan, attack and crack output will appear here as it streams in.
          </div>
        )}
      </div>
    </section>
  );
}
