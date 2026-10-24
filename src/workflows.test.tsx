// @vitest-environment jsdom
import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Scan } from "./Scan";
import { Crack } from "./Crack";
import * as bridge from "./bridge";
import { initialSnapshot } from "./types";

vi.mock("./Preflight", () => ({ Preflight: () => null }));

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

it("ignores late running snapshots after stop completes", async () => {
  let snapshot: Parameters<typeof bridge.startScan>[2] = () => {};
  vi.mocked(bridge.startScan).mockImplementationOnce(async (_i, _c, cb) => { snapshot = cb; });
  render(<Scan onBusy={vi.fn()} pushLog={vi.fn()} />);
  await screen.findByRole("option", { name: "wlan0mon (monitor)" });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Start scan" })));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Stop scan" })));
  act(() => snapshot({ ...initialSnapshot, running: true }));
  expect(screen.getByRole("button", { name: "Start scan" })).toBeTruthy();
});

it("hands selected networks to target review without losing channels", async () => {
  const targets = vi.fn();
  const networks = [1, 6].map(channel => ({ bssid: `00:11:22:33:44:0${channel}`, essid: `Lab ${channel}`, channel, power: -40, privacy: "WPA2", cipher: "CCMP", auth: "PSK", beacons: 1, dataPackets: 0, wps: false, firstSeen: 0, lastSeen: 0 }));
  vi.mocked(bridge.startScan).mockImplementationOnce(async (_i, _c, cb) => cb({ ...initialSnapshot, networks }));
  render(<Scan onBusy={vi.fn()} pushLog={vi.fn()} onTargets={targets} />);
  await screen.findByRole("option", { name: "wlan0mon (monitor)" });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Start scan" })));
  fireEvent.change(screen.getByLabelText("Selection"), { target: { value: "multiple" } });
  fireEvent.click(screen.getByLabelText("Select Lab 1"));
  fireEvent.click(screen.getByLabelText("Select Lab 6"));
  fireEvent.click(screen.getByRole("button", { name: "Review selected targets" }));
  expect(targets).toHaveBeenCalledWith(networks);
  fireEvent.change(screen.getByLabelText("Selection"), { target: { value: "single" } });
  expect(screen.getAllByRole("radio", { name: /^Select Lab/ }).filter(i => (i as HTMLInputElement).checked)).toHaveLength(1);
});

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
