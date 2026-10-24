// @vitest-environment jsdom
import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Scan } from "./Scan";
import { Crack } from "./Crack";
import * as bridge from "./bridge";
import { initialSnapshot } from "./types";

vi.mock("./bridge", () => ({
  native: true, platform: vi.fn().mockResolvedValue("linux"),
  listInterfaces: vi.fn().mockResolvedValue([{ name: "wlan0mon", interfaceType: "monitor" }]),
  startScan: vi.fn(), stopScan: vi.fn(), invoke: vi.fn().mockResolvedValue(true),
  listCaptures: vi.fn().mockResolvedValue([{ path: "/test.cap", name: "test.cap", bytes: 100, created: 0 }]),
  listWordlists: vi.fn().mockResolvedValue([{ path: "/test.txt", name: "test.txt", bytes: 100 }]),
  listRules: vi.fn().mockResolvedValue([]), startCrack: vi.fn(), stopCrack: vi.fn(),
  startHashcat: vi.fn(), exportHashcat: vi.fn(), revealCaptures: vi.fn(), hashcatInfo: vi.fn(),
}));
beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

it("allows stopping immediately after startup, even before the first snapshot", async () => {
  const busy = vi.fn();
  render(<Scan onBusy={busy} pushLog={vi.fn()} />);
  await screen.findByRole("option", { name: "wlan0mon (monitor)" });
  fireEvent.click(screen.getByRole("button", { name: "Start scan" }));
  const stop = await screen.findByRole("button", { name: "Stop scan" });
  await waitFor(() => expect((stop as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(stop);
  await screen.findByRole("button", { name: "Start scan" });
  expect(bridge.stopScan).toHaveBeenCalledOnce();
  expect(busy).toHaveBeenLastCalledWith(false);
});

it("keeps stop available when the backend rejects a stop request", async () => {
  vi.mocked(bridge.stopScan).mockRejectedValueOnce(new Error("Stop failed"));
  render(<Scan onBusy={vi.fn()} pushLog={vi.fn()} />);
  await screen.findByRole("option", { name: "wlan0mon (monitor)" });
  fireEvent.click(screen.getByRole("button", { name: "Start scan" }));
  const stop = await screen.findByRole("button", { name: "Stop scan" });
  await waitFor(() => expect((stop as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(stop);
  expect((await screen.findByRole("alert")).textContent).toContain("Stop failed");
  expect((screen.getByRole("button", { name: "Stop scan" }) as HTMLButtonElement).disabled).toBe(false);
});

it("does not overwrite a terminal scan event received before start resolves", async () => {
  vi.mocked(bridge.startScan).mockImplementationOnce(async (_i, _c, snapshot) => snapshot(initialSnapshot));
  const busy = vi.fn();
  render(<Scan onBusy={busy} pushLog={vi.fn()} />);
  await screen.findByRole("option", { name: "wlan0mon (monitor)" });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Start scan" })));
  expect(busy).toHaveBeenLastCalledWith(false);
  expect(screen.getByRole("button", { name: "Start scan" })).toBeTruthy();
});

it("shows the final recovery result and unlocks controls on completion", async () => {
  vi.mocked(bridge.startCrack).mockImplementationOnce(async (_c, _w, _b, _m, status) => {
    status({ running: false, capture: "/test.cap", mode: "auto", progress: 1, key: null, message: "Completed without finding the key." });
  });
  const busy = vi.fn();
  render(<Crack active onBusy={busy} pushLog={vi.fn()} />);
  await screen.findByRole("option", { name: /test.txt/ });
  fireEvent.change(screen.getByLabelText("CAPTURE"), { target: { value: "/test.cap" } });
  fireEvent.change(screen.getByLabelText("WORDLIST"), { target: { value: "/test.txt" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: /Start/ })));
  expect(screen.getByText("Completed without finding the key.")).toBeTruthy();
  expect(busy).toHaveBeenLastCalledWith(false);
  expect((screen.getByLabelText("CAPTURE") as HTMLSelectElement).disabled).toBe(false);
});
