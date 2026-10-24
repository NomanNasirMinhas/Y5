import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import {
  WirelessInterface,
  Network,
  Client,
  ScanSnapshot,
  LogEvent,
  CaptureInfo,
  Wordlist,
  CrackStatus,
  HashcatOptions,
  initialSnapshot,
} from "./types";

export const native = isTauri();
export { invoke };

// ---------------------------------------------------------------------------
// Native commands are thin wrappers over `invoke`. The browser preview (when
// `native` is false) is explicitly synthetic: only interface listing and the
// scan tab produce demo data; injection, cracking and monitor mode require the
// desktop app and its Linux wireless stack.
// ---------------------------------------------------------------------------

export function platform(): Promise<string> {
  if (native) return invoke<string>("platform");
  return Promise.resolve("linux");
}

export async function listInterfaces(): Promise<WirelessInterface[]> {
  if (native) return invoke("list_interfaces");
  return [
    { name: "wlan0", mac: "aa:bb:cc:dd:ee:ff", interfaceType: "managed", state: "up" },
    { name: "wlan0mon", mac: "aa:bb:cc:dd:ee:fe", interfaceType: "monitor", state: "up" },
  ];
}

export async function startMonitor(iface: string): Promise<string> {
  if (!native)
    throw new Error(
      "Monitor mode runs in the desktop app on Linux. This browser is a visual demo.",
    );
  return invoke("start_monitor", { interface: iface });
}
export async function stopMonitor(iface: string): Promise<string> {
  if (!native) throw new Error("Monitor mode runs in the desktop app on Linux.");
  return invoke("stop_monitor", { interface: iface });
}
export async function checkKill(): Promise<string> {
  if (!native) throw new Error("check-kill runs in the desktop app on Linux.");
  return invoke("check_kill");
}

async function waitForRadioIdle(kind: "scan" | "attack") {
  const deadline = Date.now() + 45000;
  while (await invoke<boolean>("job_running", { kind })) {
    if (Date.now() > deadline) throw new Error("Radio cleanup is still running. Check the activity log and retry Stop.");
    await new Promise(resolve => setTimeout(resolve, 150));
  }
}

// --- Scan -------------------------------------------------------------------

export async function startScan(
  iface: string,
  channel: number | null,
  onSnapshot: (s: ScanSnapshot) => void,
  onLog: (e: LogEvent) => void,
) {
  if (native) {
    const snapshots = new Channel<ScanSnapshot>();
    snapshots.onmessage = s => {
      if (s.running) onSnapshot(s);
      else void waitForRadioIdle("scan").then(() => onSnapshot(s)).catch(e => onLog({level:"error",message:String(e),timestamp:Date.now()}));
    };
    const logs = new Channel<LogEvent>();
    logs.onmessage = onLog;
    await invoke("start_scan", {
      interface: iface,
      channel,
      snapshots,
      logs,
    });
    return;
  }
  demoScan(iface, channel, onSnapshot, onLog);
}

export async function stopScan() {
  if (native) { await invoke("stop_scan"); await waitForRadioIdle("scan"); return; }
  if (demoTimer) clearInterval(demoTimer);
  demoTimer = undefined;
}

// --- Attacks ----------------------------------------------------------------

function logChannel(onLog: (e: LogEvent) => void) {
  const logs = new Channel<LogEvent>();
  logs.onmessage = onLog;
  return logs;
}
const injectionError = () =>
  new Error(
    "Packet injection runs in the desktop app on Linux with a compatible chipset.",
  );

export async function deauth(
  iface: string,
  bssid: string,
  client: string | null,
  count: number,
  onLog: (e: LogEvent) => void,
  channel: number,
) {
  if (!native) throw injectionError();
  await invoke("deauth", {
    interface: iface,
    bssid,
    channel,
    client,
    count,
    logs: logChannel(onLog),
  });
}
export async function startDeauth(
  iface: string,
  bssid: string,
  client: string | null,
  onLog: (e: LogEvent) => void,
  channel: number,
) {
  if (!native) throw injectionError();
  await invoke("start_deauth", {
    interface: iface,
    bssid,
    channel,
    client,
    logs: logChannel(onLog),
  });
}
export async function captureHandshake(
  iface: string,
  bssid: string,
  channel: number,
  onLog: (e: LogEvent) => void,
) {
  if (!native) throw injectionError();
  await invoke("capture_handshake", {
    interface: iface,
    bssid,
    channel,
    logs: logChannel(onLog),
  });
}
export async function capturePmkid(
  iface: string,
  bssid: string,
  channel: number,
  onLog: (e: LogEvent) => void,
) {
  if (!native) throw injectionError();
  await invoke("capture_pmkid", {
    interface: iface,
    bssid,
    channel,
    logs: logChannel(onLog),
  });
}
export async function wpsAttack(
  iface: string,
  bssid: string,
  channel: number,
  onLog: (e: LogEvent) => void,
) {
  if (!native) throw injectionError();
  await invoke("wps_attack", {
    interface: iface,
    bssid,
    channel,
    logs: logChannel(onLog),
  });
}
export async function stopAttack() {
  if (!native) return;
  await invoke("stop_attack");
  await waitForRadioIdle("attack");
}

// --- Crack ------------------------------------------------------------------

export async function startCrack(
  capture: string,
  wordlist: string,
  bssid: string | null,
  mode: string,
  onStatus: (s: CrackStatus) => void,
  onLog: (e: LogEvent) => void,
) {
  if (!native) throw new Error("Cracking runs in the desktop app.");
  const status = new Channel<CrackStatus>();
  status.onmessage = onStatus;
  await invoke("start_crack", {
    capture,
    wordlist,
    bssid,
    mode,
    status,
    logs: logChannel(onLog),
  });
}
export async function stopCrack() {
  if (!native) return;
  await invoke("stop_crack");
}
export async function startHashcat(
  opts: HashcatOptions,
  onStatus: (s: CrackStatus) => void,
  onLog: (e: LogEvent) => void,
) {
  if (!native) throw new Error("Hashcat runs in the desktop app.");
  const status = new Channel<CrackStatus>();
  status.onmessage = onStatus;
  await invoke("start_hashcat", {
    opts,
    status,
    logs: logChannel(onLog),
  });
}
export async function hashcatInfo(): Promise<string> {
  if (!native) throw new Error("Device info runs in the desktop app.");
  return invoke("hashcat_info");
}
export async function exportHashcat(capture: string): Promise<string> {
  if (!native) throw new Error("Hashcat export runs in the desktop app.");
  return invoke("export_hashcat", { capture });
}

// --- Library ----------------------------------------------------------------

export async function listWordlists(): Promise<Wordlist[]> {
  if (native) return invoke("list_wordlists");
  return [];
}
export async function listRules(): Promise<Wordlist[]> {
  if (native) return invoke("list_rules");
  return [];
}
export async function listCaptures(): Promise<CaptureInfo[]> {
  if (native) return invoke("list_captures");
  return [];
}
export async function revealCaptures() {
  if (!native) return;
  await invoke("reveal_captures");
}

// --- Browser demo (synthetic) ----------------------------------------------

let demoTimer: ReturnType<typeof setInterval> | undefined;
let demoState = { ...initialSnapshot };

const DEMO_APS = [
  { bssid: "AA:BB:CC:00:11:22", essid: "HomeBase-5G", channel: 36, privacy: "WPA2", cipher: "CCMP", auth: "PSK", power: -42 },
  { bssid: "AA:BB:CC:00:11:33", essid: "CoffeeShop-Guest", channel: 6, privacy: "OPN", cipher: "", auth: "", power: -58 },
  { bssid: "AA:BB:CC:00:11:44", essid: "IoT-Network", channel: 1, privacy: "WPA3", cipher: "CCMP", auth: "SAE", power: -67 },
  { bssid: "AA:BB:CC:00:11:55", essid: "NeighborsWiFi", channel: 11, privacy: "WPA2", cipher: "CCMP", auth: "PSK", power: -74 },
  { bssid: "AA:BB:CC:00:11:66", essid: "LegacyAP", channel: 6, privacy: "WEP", cipher: "WEP", auth: "WEP", power: -81 },
];

function demoScan(
  iface: string,
  channel: number | null,
  onSnapshot: (s: ScanSnapshot) => void,
  onLog: (e: LogEvent) => void,
) {
  const t0 = Date.now();
  demoState = {
    running: true,
    interface: iface,
    channel: channel ?? 0,
    networks: [],
    clients: [],
    message: "SIMULATED SCAN · Browser preview · no radio measurements",
  };
  const log = (message: string, level = "info") =>
    onLog({ level, message, timestamp: Date.now() });
  log("SIMULATED SCAN · synthetic networks only");
  if (demoTimer) clearInterval(demoTimer);
  demoTimer = setInterval(() => {
    const now = Date.now();
    const hop = channel ?? (36 + Math.floor((now / 1200) % 5));
    const networks: Network[] = DEMO_APS.map((ap, i) => {
      const ageMs = (now - t0) % 9000;
      const visible = ageMs < 7000;
      const old = demoState.networks.find((n) => n.bssid === ap.bssid);
      const jitter = Math.round(Math.sin(now / 700 + i) * 3);
      return {
        bssid: ap.bssid,
        essid: ap.essid,
        channel: ap.channel,
        privacy: ap.privacy,
        cipher: ap.cipher,
        auth: ap.auth,
        power: visible ? ap.power + jitter : -100,
        beacons: (old?.beacons ?? 0) + 3,
        dataPackets: (old?.dataPackets ?? 0) + 1,
        firstSeen: old?.firstSeen ?? now,
        lastSeen: now,
        wps: i === 3,
      };
    });
    const clients: Client[] = [
      {
        mac: "11:22:33:44:55:66",
        bssid: DEMO_APS[0].bssid,
        power: -35,
        packets: 120 + Math.floor((now / 1000) % 60),
        probedEssids: "HomeBase-5G",
        firstSeen: demoState.clients[0]?.firstSeen ?? now,
        lastSeen: now,
      },
      {
        mac: "22:33:44:55:66:77",
        bssid: DEMO_APS[1].bssid,
        power: -60,
        packets: 14 + Math.floor((now / 3000) % 10),
        probedEssids: "CoffeeShop-Guest",
        firstSeen: demoState.clients[1]?.firstSeen ?? now,
        lastSeen: now,
      },
    ];
    demoState = {
      ...demoState,
      channel: hop,
      networks,
      clients,
      message: "SIMULATED SCAN · Browser preview · no radio measurements",
    };
    onSnapshot(demoState);
  }, 400);
}
