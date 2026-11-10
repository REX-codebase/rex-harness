import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { terminalSpawn, terminalWrite, terminalResize, terminalKill, onTerminalOutput, onTerminalExit } from "../data/terminal";
import { errorText } from "../data/runLifecycle";
import { useWorkspace } from "../data/workspaceContext";
import { NoWorkspace } from "./NoWorkspace";

type Status = "starting" | "running" | "exited" | "failed";

// An interactive terminal in the open folder. The shell lives in the Rust
// backend (folder vetted against the ones you opened, environment rebuilt from
// an allowlist); xterm.js only draws it.
//
// The app keeps this mounted once it has been shown, so switching to another
// tab and back does not kill your shell or lose its scrollback. `active` says
// whether it is the tab on screen, which is when it needs to refit and focus.
export function TerminalView({ active }: { active: boolean }) {
  const ws = useWorkspace();
  const workspace = ws.active?.path ?? null;
  if (!workspace) return active ? <NoWorkspace feature="terminal" /> : null;
  // a new folder is a new shell, so the session is keyed by folder
  return <TerminalSession key={workspace} workspace={workspace} folderName={ws.active?.name ?? ""} active={active} />;
}

function TerminalSession({ workspace, folderName, active }: { workspace: string; folderName: string; active: boolean }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const idRef = useRef<string | null>(null);
  const activeRef = useRef(active);
  activeRef.current = active;
  const [status, setStatus] = useState<Status>("starting");
  const [detail, setDetail] = useState<string | null>(null);
  // Bumping this starts a fresh shell in the same folder ("Restart").
  const [session, setSession] = useState(0);

  const refit = useCallback(() => {
    const term = termRef.current;
    const fit = fitRef.current;
    const el = containerRef.current;
    // a hidden tab has no size; fitting it would squash the shell to nothing
    if (!term || !fit || !el || el.clientWidth === 0 || el.clientHeight === 0) return;
    try {
      fit.fit();
    } catch {
      return;
    }
    const id = idRef.current;
    if (id) terminalResize(id, term.cols, term.rows).catch(() => undefined);
  }, []);

  useEffect(() => {
    let alive = true;
    let unlistenOutput: (() => void) | null = null;
    let unlistenExit: (() => void) | null = null;
    let observer: ResizeObserver | null = null;
    const el = containerRef.current;
    if (!el) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      scrollback: 5000,
      theme: { background: "#0d1117", foreground: "#e6edf3", cursor: "#e6edf3", selectionBackground: "#264f78" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    termRef.current = term;
    fitRef.current = fit;
    setStatus("starting");
    setDetail(null);

    const start = async () => {
      try {
        if (el.clientWidth > 0) fit.fit();
        const id = await terminalSpawn(workspace, term.cols, term.rows);
        if (!alive) {
          await terminalKill(id).catch(() => undefined);
          return;
        }
        idRef.current = id;
        unlistenOutput = await onTerminalOutput(id, (data) => term.write(data));
        unlistenExit = await onTerminalExit(id, () => {
          if (!alive) return;
          idRef.current = null;
          setStatus("exited");
        });
        term.onData((data) => {
          const live = idRef.current;
          if (live) terminalWrite(live, data).catch(() => undefined);
        });
        setStatus("running");
        if (activeRef.current) term.focus();
        observer = new ResizeObserver(() => refit());
        observer.observe(el);
      } catch (e) {
        if (alive) {
          setStatus("failed");
          setDetail(errorText(e));
        }
      }
    };
    void start();

    return () => {
      alive = false;
      observer?.disconnect();
      unlistenOutput?.();
      unlistenExit?.();
      const id = idRef.current;
      idRef.current = null;
      if (id) terminalKill(id).catch(() => undefined);
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [workspace, session, refit]);

  // Coming back to the tab: the container has a size again, so fit and focus.
  useEffect(() => {
    if (!active) return;
    const frame = window.requestAnimationFrame(() => {
      refit();
      termRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [active, refit]);

  return (
    <div className="terminal-view" aria-label="Workspace terminal" hidden={!active}>
      <div className="terminal-view-bar">
        <span className="eyebrow">Terminal</span>
        <span className="terminal-view-folder" title={workspace}>
          {folderName}
        </span>
        <span className={`terminal-view-state is-${status}`} role="status">
          {status === "starting" ? "starting…" : status === "running" ? "running" : status === "exited" ? "shell exited" : "could not start"}
        </span>
        {(status === "exited" || status === "failed") && (
          <button type="button" className="reset-button" onClick={() => setSession((n) => n + 1)}>
            Restart
          </button>
        )}
      </div>
      {status === "failed" && (
        <p className="terminal-view-error" role="alert">
          Could not open a terminal in this folder: {detail}
        </p>
      )}
      <div ref={containerRef} className="terminal-view-body" />
    </div>
  );
}
