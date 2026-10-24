export interface WirelessInterface {
  name: string;
  mac: string;
  interfaceType: string;
  state: string;
}

export interface Network {
  bssid: string;
  essid: string;
  channel: number;
  privacy: string;
  cipher: string;
  auth: string;
  power: number;
  beacons: number;
  dataPackets: number;
  firstSeen: number;
  lastSeen: number;
  wps: boolean;
}

export interface Client {
  mac: string;
  bssid: string;
  power: number;
  packets: number;
  probedEssids: string;
  firstSeen: number;
  lastSeen: number;
}

export interface ScanSnapshot {
  running: boolean;
  interface: string;
  channel: number;
  networks: Network[];
  clients: Client[];
  message: string;
}

export interface LogEvent {
  level: string;
  message: string;
  timestamp: number;
}

export interface CaptureInfo {
  path: string;
  name: string;
  bytes: number;
  created: number;
}

export interface Wordlist {
  path: string;
  name: string;
  bytes: number;
}

export interface CrackStatus {
  running: boolean;
  capture: string;
  mode: string;
  progress: number;
  key: string | null;
  message: string;
  /** Hashcat engine extras: last reported cracking speed and recovered/total. */
  speed?: string | null;
  recovered?: string | null;
}

export interface HashcatOptions {
  capture: string;
  hashMode: number | null;
  attack: "wordlist" | "rules" | "mask";
  wordlist?: string | null;
  rules?: string | null;
  mask?: string | null;
  increment: boolean;
}

export const initialSnapshot: ScanSnapshot = {
  running: false,
  interface: "",
  channel: 0,
  networks: [],
  clients: [],
  message: "Choose an interface and start a scan to discover nearby networks.",
};

/** Seconds since `time`, as a compact human label (mirrors RF Radar's age()). */
export const age = (time: number, now = Date.now()) => {
  const s = Math.max(0, Math.floor((now - time) / 1000));
  return s < 2
    ? "just now"
    : s < 60
      ? `${s}s ago`
      : s < 3600
        ? `${Math.floor(s / 60)}m ago`
        : `${Math.floor(s / 3600)}h ago`;
};

export const fmtBytes = (n: number) => {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
};

/** Map an airodump-ng privacy token to a badge label + class. */
export function encryption(privacy: string): { label: string; kind: string } {
  const p = (privacy || "").toUpperCase();
  if (p.includes("WPA3")) return { label: "WPA3", kind: "wpa3" };
  if (p.includes("WPA2")) return { label: "WPA2", kind: "wpa2" };
  if (p.includes("WPA")) return { label: "WPA", kind: "wpa" };
  if (p.includes("WEP")) return { label: "WEP", kind: "wep" };
  if (p.includes("OPN")) return { label: "OPEN", kind: "open" };
  return { label: privacy || "—", kind: "unknown" };
}

/** airodump-ng power is roughly -100 (weak) … -30 (strong) dBm. */
export function signalStrength(power: number): number {
  return Math.max(0, Math.min(1, (power + 100) / 70));
}
