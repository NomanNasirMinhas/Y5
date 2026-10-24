import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  Wifi,
  Crosshair,
  KeyRound,
  Network,
  ScrollText,
  ChevronRight,
  HelpCircle,
  Radio,
  RefreshCw,
  X,
} from "lucide-react";
import { native, platform } from "./bridge";
import { LogEvent } from "./types";
import "./style.css";
import { Scan } from "./Scan";
import { Attacks } from "./Attacks";
import { Crack } from "./Crack";
import { Interfaces } from "./Interfaces";
import { Log } from "./Log";
import { UpdateDialog } from "./UpdateDialog";
declare const __APP_VERSION__: string;

type Tab = "scan" | "attacks" | "crack" | "interfaces" | "log";

const TABS: {
  id: Tab;
  label: string;
  hint: string;
  icon: React.ReactNode;
  eyebrow: string;
  title: string;
  blurb: string;
  crumb: string;
}[] = [
  {
    id: "scan",
    label: "Scan",
    hint: "Discover networks & clients",
    icon: <Crosshair size={18} />,
    eyebrow: "PASSIVE DISCOVERY",
    title: "Discover the airwaves.",
    blurb:
      "Survey nearby access points and their clients with a passive monitor-mode scan.",
    crumb: "Network discovery",
  },
  {
    id: "attacks",
    label: "Attacks",
    hint: "Deauth · handshake · PMKID · WPS",
    icon: <Wifi size={18} />,
    eyebrow: "AUTHORIZED TARGETS ONLY",
    title: "Run targeted attacks.",
    blurb:
      "Deauthentication, handshake and PMKID capture, and WPS recovery — on your own network.",
    crumb: "Targeted attacks",
  },
  {
    id: "crack",
    label: "Crack",
    hint: "aircrack-ng & hashcat export",
    icon: <KeyRound size={18} />,
    eyebrow: "OFFLINE RECOVERY",
    title: "Crack captured keys.",
    blurb:
      "Run aircrack-ng against a capture with a wordlist, or export for hashcat.",
    crumb: "Offline recovery",
  },
  {
    id: "interfaces",
    label: "Interfaces",
    hint: "Monitor-mode management",
    icon: <Network size={18} />,
    eyebrow: "RADIO SETUP",
    title: "Manage your radios.",
    blurb:
      "List wireless interfaces and toggle monitor mode for packet capture.",
    crumb: "Radio setup",
  },
  {
    id: "log",
    label: "Log",
    hint: "Unified event stream",
    icon: <ScrollText size={18} />,
    eyebrow: "SESSION LOG",
    title: "Every event, in order.",
    blurb:
      "The unified stream of scan, attack and crack output.",
    crumb: "Event log",
  },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("scan");
  const [logs, setLogs] = useState<LogEvent[]>([]);
  const [busyScan, setBusyScan] = useState(false);
  const [busyAttack, setBusyAttack] = useState(false);
  const [busyCrack, setBusyCrack] = useState(false);
  const [os, setOs] = useState("");
  const [help, setHelp] = useState(false);
  const guide = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (help) guide.current?.showModal();
  }, [help]);

  useEffect(() => {
    platform()
      .then((p) => setOs(p))
      .catch(() => {});
  }, []);

  const pushLog = useCallback((e: LogEvent) => {
    setLogs((p) => [...p, e].slice(-800));
  }, []);
  const busy = busyScan || busyAttack || busyCrack;
  const current = TABS.find((t) => t.id === tab)!;
  const linux = os === "linux" || !native;
  const headerStatus = !native
    ? "DEMO · SYNTHETIC"
    : linux
      ? "LINUX · FULL SUITE"
      : `${os.toUpperCase()} · PASSIVE + OFFLINE`;

  return (
    <div className="shell">
      <UpdateDialog busy={busy} />
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          aria-label="Y5 home"
          onClick={() => setTab("scan")}
        >
          <span className="brand-mark">
            <Wifi size={25} />
          </span>
          <span>
            Y<span className="brand-light">5</span>
            <small>Airwave Audit Console</small>
          </span>
        </a>
        <div className="nav-caption">WORKSPACE</div>
        <nav>
          {TABS.map((t) => (
            <button
              key={t.id}
              className={tab === t.id ? "active" : ""}
              aria-current={tab === t.id ? "page" : undefined}
              onClick={() => setTab(t.id)}
            >
              {t.icon}
              <span>
                {t.label}
                <small style={{ display: "block", fontSize: 8, color: "var(--muted)", fontWeight: 400 }}>
                  {t.hint}
                </small>
              </span>
            </button>
          ))}
          <button onClick={() => setHelp(true)}>
            <HelpCircle size={18} />
            <span>
              Guide
              <small style={{ display: "block", fontSize: 8, color: "var(--muted)", fontWeight: 400 }}>
                Authorized testing
              </small>
            </span>
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="receiver-card">
            <div className="eyebrow">PLATFORM</div>
            <Radio size={25} />
            <strong>{!native ? "Simulation source" : os.toUpperCase()}</strong>
            <span>
              {linux
                ? "aircrack-ng suite · injection"
                : "passive scan + offline crack"}
            </span>
            <div className={`connection ${linux ? "online" : ""}`}>
              <i />
              {!native ? "Demo mode" : linux ? "Verify radio setup" : "Limited mode"}
            </div>
            <button className="text-button" onClick={() => setTab("interfaces")}>
              <RefreshCw size={12} />
              Manage interfaces
            </button>
          </div>
          <div className="local">
            <span />
            LOCAL PROCESSING <small>v{__APP_VERSION__}</small>
          </div>
        </div>
      </aside>
      <main>
        <header>
          <div className="breadcrumb">
            Workspace
            <ChevronRight size={12} />
            <span>{current.crumb}</span>
          </div>
          <div className="header-right">
            <span className={`small-status ${linux ? "" : "offline"}`}>
              <i />
              {headerStatus}
            </span>
            <button
              className="icon-button"
              title="Authorized testing guide"
              onClick={() => setHelp(true)}
            >
              <HelpCircle size={18} />
            </button>
          </div>
        </header>
        <div className="page">
          <div className="title-row">
            <div>
              <div className="eyebrow orange">{current.eyebrow}</div>
              <h1>{current.title}</h1>
              <p>{current.blurb}</p>
            </div>
            <div className={`source-badge ${!native ? "demo" : ""}`}>
              <span />
              {!native ? "DEMO · SYNTHETIC" : linux ? "LINUX · LOCAL" : "PASSIVE + OFFLINE"}
            </div>
          </div>
          <div hidden={tab !== "scan"}>
            <Scan visible={tab === "scan"} onBusy={setBusyScan} pushLog={pushLog} />
          </div>
          <div hidden={tab !== "attacks"}>
            <Attacks visible={tab === "attacks"} onBusy={setBusyAttack} pushLog={pushLog} />
          </div>
          <div hidden={tab !== "crack"}>
            <Crack
              onBusy={setBusyCrack}
              pushLog={pushLog}
              active={tab === "crack"}
            />
          </div>
          <div hidden={tab !== "interfaces"}>
            <Interfaces busy={busy} />
          </div>
          <div hidden={tab !== "log"}>
            <Log logs={logs} onClear={() => setLogs([])} />
          </div>
          <footer>
            <span className={`status-led ${busy ? "on" : ""}`} />
            <span>
              {busy
                ? "A job is running. Stop it before closing the app."
                : "Authorized security testing only. Run against networks you own."}
            </span>
            <span className="footer-end">
              {native ? "NATIVE ENGINE" : "BROWSER PREVIEW"}
              <span>•</span>
              LOCAL PROCESSING
            </span>
          </footer>
        </div>
      </main>
      {help && (
        <dialog ref={guide} className="guide-dialog" onCancel={() => setHelp(false)} onClick={(e) => { if (e.target === e.currentTarget) setHelp(false); }}>
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label="Authorized testing guide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="panel-heading">
              <h2>
                <HelpCircle size={18} />
                Authorized testing guide
              </h2>
              <button
                className="icon-button"
                aria-label="Close guide"
                onClick={() => setHelp(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="guide-body">
              <p>
                <b>Authorized targets only.</b> Deauthentication, handshake
                capture and cracking disrupt wireless networks and can expose
                keys. Use Y5 only on networks you own, or where you
                have explicit written permission to test.
              </p>
              <p>
                <b>Linux for the full suite.</b> Monitor mode and packet
                injection require Linux with a compatible chipset (Atheros,
                Ralink or Realtek). Windows and macOS builds still scan
                passively where the OS allows it and can crack existing
                captures offline.
              </p>
              <p>
                <b>Everything runs locally.</b> Y5 shells out to your
                system-installed aircrack-ng suite. No captures, keys or
                packets leave this computer.
              </p>
              <div className="guide-mode">
                <b>Scan</b>
                <span>Passive discovery with airodump-ng — networks and clients
                in range.</span>
              </div>
              <div className="guide-mode">
                <b>Attacks</b>
                <span>Deauth (aireplay-ng), handshake capture, PMKID
                (hcxdumptool) and WPS (reaver/bully).</span>
              </div>
              <div className="guide-mode">
                <b>Crack</b>
                <span>Offline recovery with aircrack-ng, plus hashcat export.</span>
              </div>
              <p>
                <b>Observed, not identified.</b> MAC addresses and ESSIDs are
                broadcast metadata; the app does not claim to name any device or
                person.
              </p>
            </div>
          </section>
        </dialog>
      )}
    </div>
  );
}
