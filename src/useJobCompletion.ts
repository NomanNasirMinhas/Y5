import { useEffect, useRef } from "react";
import { invoke, native } from "./bridge";

/** Reconcile jobs that exit without a final channel event, including spawn errors. */
export function useJobCompletion(
  kind: "scan" | "attack" | "crack",
  running: boolean,
  complete: () => void,
  failed: (message: string) => void,
) {
  const callbacks = useRef({ complete, failed });
  callbacks.current = { complete, failed };
  useEffect(() => {
    if (!native || !running) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const active = await invoke<boolean>("job_running", { kind });
        if (cancelled) return;
        if (!active) {
          callbacks.current.complete();
          return;
        }
      } catch (e) {
        if (!cancelled) callbacks.current.failed(`Unable to verify job state: ${String(e)}`);
      }
      if (!cancelled) timer = setTimeout(poll, 1000);
    };
    timer = setTimeout(poll, 1000);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [kind, running]);
}
