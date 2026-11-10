import { useEffect, useRef, useState } from "react";
import { useSlideIndicator } from "./Fx";
import { SearchProviderSettings } from "./SearchProviderSettings";
import { InstalledAgentsSettings } from "./InstalledAgentsSettings";
import { FableMcpSettings } from "./FableMcpSettings";
import { McpServerPanel } from "./McpServerPanel";
import { CostPanel } from "./CostPanel";
import { isDesktopRuntime } from "./DesktopOnly";
import { appInfo, type AppInfo } from "../data/workspace";
import type { MotionPref } from "../data/motion";
export type { MotionPref };
const MOTION_OPTIONS: { id: MotionPref; label: string; hint: string }[] = [
  { id: "system", label: "System", hint: "Follow the device setting" },
  { id: "reduce", label: "Reduce", hint: "Minimal animation" },
  { id: "full", label: "Full", hint: "All transitions" },
];
// Where this install keeps its data, for support and for the person who wants to
// back it up or delete it. The desktop app reports the real paths.
function AboutSection() {
  const desktop = isDesktopRuntime();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    appInfo()
      .then((i) => {
        if (alive) setInfo(i);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [desktop]);
  const copy = async (label: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(label);
      window.setTimeout(() => setCopied((c) => (c === label ? null : c)), 1800);
    } catch {
      /* clipboard unavailable; the path is still selectable on screen */
    }
  };
  const row = (label: string, value: string, copyable = false) => (
    <div className="settings-row settings-row-wrap about-row" key={label}>
      <span className="text-sm text-text">{label}</span>
      <span className="about-value">
        <code title={value}>{value}</code>
        {copyable && (
          <button type="button" className="reset-button" onClick={() => void copy(label, value)} aria-label={`Copy ${label.toLowerCase()}`}>
            {copied === label ? "Copied" : "Copy"}
          </button>
        )}
      </span>
    </div>
  );
  return (
    <section aria-label="About" className="settings-section">
      <h2 className="eyebrow">About</h2>
      {row("Version", info?.version ?? __APP_VERSION__)}
      {desktop && info && (
        <>
          {row("Your data", info.data_dir, true)}
          {row("Run sandboxes", info.runs_root, true)}
          {row("Git", info.git_available ? "Found on this computer" : "Not installed")}
        </>
      )}
      {desktop && !info && !failed && <p className="settings-note" role="status">Reading this install's details…</p>}
      {desktop && failed && <p className="settings-note">This install's details could not be read.</p>}
      <p className="settings-note">
        {desktop
          ? "Keys, your ChatGPT sign-in, run history and receipts live in the data folder, on this computer only. Deleting the folder erases them; uninstalling the app does not."
          : "You are in the browser preview. The desktop app keeps your data on your computer and has no server."}
      </p>
    </section>
  );
}

export function SettingsView({ motion, setMotion, onReset }: { motion: MotionPref; setMotion: (m: MotionPref) => void; onReset: () => void }) {
  const segRef = useRef<HTMLDivElement>(null);
  useSlideIndicator(segRef, motion, ".is-on");
  return <div className="settings-view mt-3 sm:mt-7">
    <div className="mb-6 sm:mb-8"><p className="eyebrow">Settings</p><h1 className="mt-2 text-[30px] font-medium leading-[1.12] tracking-[-0.04em] text-text sm:text-[38px]">Quiet defaults, honest state.</h1></div>
    <section aria-label="Runtime" className="settings-section"><h2 className="eyebrow">Runtime</h2><div className="settings-row settings-row-wrap"><span className="flex items-center gap-3"><span className="h-1.5 w-1.5 rounded-full bg-done" aria-hidden="true" /><span className="text-sm text-text">Local tools protected</span></span><span className="text-xs text-faint">Workspace only</span></div><p className="settings-note">Each run works in its own sandbox folder. File changes and commands pause for your approval, unless you start the run in Autonomous mode, which removes the prompts but none of the limits. Paths outside the sandbox, symlink escapes, risky commands, and unrestricted shell access are blocked in Rust.</p></section>
    <InstalledAgentsSettings />
    <SearchProviderSettings />
    <FableMcpSettings />
    <McpServerPanel />
    {isDesktopRuntime() ? (
      <CostPanel />
    ) : (
      <section aria-label="Cost" className="settings-section">
        <h2 className="eyebrow">Cost</h2>
        <p className="settings-desktop-note">Spend, budgets and per-run receipts are kept by the desktop app in a local cost.json file. Open REX with <code>npm run tauri dev</code> to see them.</p>
      </section>
    )}
    <section aria-label="Appearance" className="settings-section"><h2 className="eyebrow">Appearance</h2><div className="settings-row settings-row-wrap"><span className="text-sm text-text">Motion</span><div ref={segRef} className="seg-control" role="radiogroup" aria-label="Motion preference">{MOTION_OPTIONS.map((option) => <button key={option.id} type="button" role="radio" aria-checked={motion === option.id} title={option.hint} className={`seg-option ${motion === option.id ? "is-on" : ""}`} onClick={() => setMotion(option.id)}>{option.label}</button>)}</div></div><p className="settings-note">Applies immediately and is saved on this device only.</p></section>
    <section aria-label="Keyboard shortcuts" className="settings-section"><h2 className="eyebrow">Keyboard</h2><dl className="shortcut-list"><div><dt>Ctrl + Enter</dt><dd>Run the task, send a follow-up, or send a typed answer</dd></div><div><dt>Alt + P</dt><dd>Switch the new task between Build and Plan</dd></div><div><dt>Ctrl + S</dt><dd>Save the open file in the Editor</dd></div><div><dt>Ctrl + Enter in a commit message</dt><dd>Commit the staged files in the Git tab</dd></div><div><dt>Esc</dt><dd>Close a menu or dialog; deny an approval or reject a plan</dd></div><div><dt>Tab</dt><dd>Move through controls</dd></div></dl></section>
    <section aria-label="Preferences" className="settings-section"><h2 className="eyebrow">Preferences</h2><div className="settings-row settings-row-wrap"><span className="text-sm text-text">Motion preference</span><button type="button" className="reset-button" onClick={onReset}>Reset to defaults</button></div><p className="settings-note">Puts the motion setting back to following your device. Your keys, models, folders and run history are not touched.</p></section>
    <section aria-label="Support" className="settings-section"><h2 className="eyebrow">Support</h2><p className="settings-note">REX Harness is built independently by one student. If it is useful to you, a <a className="support-link" href="https://github.com/REX-codebase/rex-harness" target="_blank" rel="noreferrer">star on GitHub</a> helps REX grow. If you can, a <a className="support-link" href="https://github.com/sponsors/REX-codebase" target="_blank" rel="noreferrer">donation</a> supports the independent student building it.</p></section>
    <AboutSection />
    <p className="mt-8 text-xs leading-relaxed text-faint">REX Harness v{__APP_VERSION__} · {isDesktopRuntime() ? "Local tools are enforced by the desktop runtime." : "Browser preview."}</p>
  </div>;
}
