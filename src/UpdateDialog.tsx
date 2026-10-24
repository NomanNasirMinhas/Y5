import React, { useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { invoke, native } from "./bridge";
import { Download, RefreshCw } from "lucide-react";
import "./updates.css";

type Update = { version: string; currentVersion: string; notes: string };
type Progress = {
  phase: "downloading" | "installing";
  downloaded: number;
  total: number | null;
};
// Share the startup request across React StrictMode's effect remount.
let startupCheck: Promise<Update | null> | undefined;

export function UpdateDialog({ busy }: { busy: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const lock = useRef(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [updating, setUpdating] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!native) return;
    let active = true;
    startupCheck ??= invoke<Update | null>("check_update");
    startupCheck
      .then((value) => {
        if (active) setUpdate(value);
      })
      .catch((error) =>
        console.info("Startup update check unavailable:", error),
      );
    return () => {
      active = false;
    };
  }, []);
  useEffect(() => {
    if (update && !dialog.current?.open) dialog.current?.showModal();
  }, [update]);
  function dismiss() {
    if (lock.current) return;
    dialog.current?.close();
    setUpdate(null);
  }
  async function install() {
    if (lock.current || busy) return;
    lock.current = true;
    setUpdating(true);
    setError("");
    setProgress({ phase: "downloading", downloaded: 0, total: null });
    const events = new Channel<Progress>();
    events.onmessage = setProgress;
    try {
      await invoke("install_update", { events });
    } catch (e) {
      setError(
        `Update could not be completed: ${String(e)}. You can retry or restart Y5.`,
      );
      setUpdating(false);
      lock.current = false;
      setProgress(null);
    }
  }
  if (!update) return null;
  const percent = progress?.total
    ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
    : undefined;
  return (
    <dialog
      ref={dialog}
      className="update-dialog"
      aria-labelledby="update-title"
      aria-describedby="update-description"
      onCancel={(e) => {
        e.preventDefault();
        dismiss();
      }}
    >
      <div className="update-mark">
        <Download size={26} />
      </div>
      <h2 id="update-title">A new Y5 is ready</h2>
      <p id="update-description">
        Update from {update.currentVersion} to <strong>{update.version}</strong>
        .
      </p>
      {update.notes && <p className="update-notes">{update.notes}</p>}
      <p>
        Your captures stay on this computer. Y5 will close and
        restart to finish updating.
      </p>
      {busy && (
        <p className="update-warning" role="status">
          Stop active scans or attacks before updating.
        </p>
      )}
      {updating && (
        <div role="status" aria-live="polite">
          <p>
            {progress?.phase === "installing"
              ? "Installing update…"
              : `Downloading update${percent === undefined ? "…" : ` · ${percent}%`}`}
          </p>
          <progress
            max={100}
            value={progress?.phase === "installing" ? undefined : percent}
            aria-label="Update progress"
          />
        </div>
      )}
      {error && (
        <p className="update-warning" role="alert">
          {error}
        </p>
      )}
      <div className="update-actions">
        <button onClick={dismiss} disabled={updating} autoFocus>
          Later
        </button>
        <button
          className="primary"
          onClick={install}
          disabled={updating || busy}
        >
          <RefreshCw size={15} />{" "}
          {updating ? "Updating…" : error ? "Retry update" : "Update and restart"}
        </button>
      </div>
    </dialog>
  );
}
