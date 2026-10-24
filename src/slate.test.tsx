// @vitest-environment jsdom
import React from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Log } from "./Log";
import { Preflight } from "./Preflight";
import { invoke } from "./bridge";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./bridge", () => ({ native: true, invoke: vi.fn(async (command: string) => command === "preflight" ? [{name:"iw",ok:true,required:true,detail:"/usr/bin/iw"}] : undefined) }));
afterEach(cleanup);

it("filters the activity console and clears through its parent", () => {
  const clear = vi.fn();
  render(<Log logs={[{level:"info",message:"Scan started",timestamp:0},{level:"error",message:"Adapter disconnected",timestamp:0}]} onClear={clear}/>);
  fireEvent.change(screen.getByLabelText("Log level"), {target:{value:"error"}});
  expect(screen.queryByText("Scan started")).toBeNull();
  expect(screen.getByText("Adapter disconnected")).toBeTruthy();
  fireEvent.click(screen.getByRole("button",{name:"Clear"}));
  expect(clear).toHaveBeenCalledOnce();
});

it("writes the restore-on-exit option to the native backend", async () => {
  render(<Preflight interfaceName="wlan0mon"/>);
  await screen.findByText("All core checks passed");
  fireEvent.click(screen.getByRole("checkbox",{name:/Restore on exit/}));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_restore_on_exit", {enabled:false}));
  await waitFor(() => expect((screen.getByRole("checkbox",{name:/Restore on exit/}) as HTMLInputElement).checked).toBe(false));
});
