import { afterEach, expect, it, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: vi.fn(), Channel: class {} }));
import { invoke } from "@tauri-apps/api/core";
import { stopAttack, stopScan } from "./bridge";
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks(); });
it.each([["attack", stopAttack], ["scan", stopScan]] as const)("keeps %s stopping until cleanup finishes", async (kind, stop) => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockResolvedValueOnce(undefined).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
  let finished = false;
  const request = stop().then(() => { finished = true; });
  await vi.advanceTimersByTimeAsync(0);
  expect(finished).toBe(false);
  expect(invoke).toHaveBeenCalledWith("job_running", {kind});
  await vi.advanceTimersByTimeAsync(150);
  await request;
  expect(finished).toBe(true);
});
