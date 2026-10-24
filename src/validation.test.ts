import { expect, it } from "vitest";
import { validMac, validateTarget } from "./validation";

it("validates target addresses and rejects invalid numeric input", () => {
  expect(validMac(" aa:bb:cc:dd:ee:ff ")).toBe(true);
  expect(validMac("--help")).toBe(false);
  const bssid = "AA:BB:CC:DD:EE:FF";
  expect(validateTarget(bssid, "", "6", "10")).toBe("");
  for (const channel of ["", "0", "166", "1.5", "NaN"]) expect(validateTarget(bssid, "", channel, "10")).not.toBe("");
  for (const count of ["0", "-1", "10000", "2.5"]) expect(validateTarget(bssid, "", "6", count)).not.toBe("");
  expect(validateTarget(bssid, "invalid", "6", "10")).toContain("client MAC");
});
