import { describe, it, expect } from "vitest";
import { age, encryption, fmtBytes, signalStrength } from "./types";

describe("age", () => {
  it("labels recent and older timestamps", () => {
    const now = 1_000_000;
    expect(age(now, now)).toBe("just now");
    expect(age(now - 5_000, now)).toBe("5s ago");
    expect(age(now - 125_000, now)).toBe("2m ago");
    expect(age(now - 7_200_000, now)).toBe("2h ago");
  });
});

describe("encryption", () => {
  it("maps airodump privacy tokens to badges", () => {
    expect(encryption("WPA2").label).toBe("WPA2");
    expect(encryption("WPA3").kind).toBe("wpa3");
    expect(encryption("OPN").kind).toBe("open");
    expect(encryption("WEP").kind).toBe("wep");
    expect(encryption("").label).toBe("—");
  });
});

describe("fmtBytes", () => {
  it("formats sizes", () => {
    expect(fmtBytes(512)).toBe("512 B");
    expect(fmtBytes(2048)).toBe("2.0 KB");
    expect(fmtBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });
});

describe("signalStrength", () => {
  it("clamps dBm to 0..1", () => {
    expect(signalStrength(-30)).toBe(1);
    expect(signalStrength(-100)).toBe(0);
    expect(signalStrength(-65)).toBeCloseTo(0.5, 1);
  });
});
