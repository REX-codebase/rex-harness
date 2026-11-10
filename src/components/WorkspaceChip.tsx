import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../data/workspaceContext";
import { gitChip } from "../data/git";
import { runSandboxLabel } from "../data/workspaceLabels";

// The folder the Terminal, Editor and Git tabs are working in, with a menu to
// open another. Desktop only: a browser tab has no folders to open.
export function WorkspaceChip() {
  const ws = useWorkspace();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!ws.desktop) return null;
  const recent = ws.snapshot?.recent ?? [];
  const runs = ws.snapshot?.runs ?? [];
  const active = ws.active;

  return (
    <div className="workspace-chip" ref={rootRef}>
      <button
        type="button"
        className={`workspace-button ${open ? "is-open" : ""}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={active ? `Folder: ${active.name}. Change folder` : "Open a folder"}
        title={active ? active.path : "Open a project folder for the terminal, editor and git"}
        onClick={() => setOpen((v) => !v)}
      >
        <svg width="14" height="14" viewBox="0 0 22 22" aria-hidden="true"><path d="M2.5 6.5a2 2 0 0 1 2-2h4l2 2.2h7a2 2 0 0 1 2 2v7.3a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" /></svg>
        <span className="workspace-name">{active ? active.name : "Open a folder"}</span>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
      </button>
      {open && (
        <div className="workspace-menu" role="dialog" aria-label="Folders">
          {ws.error && <p className="workspace-menu-error" role="alert">{ws.error}</p>}
          {recent.length > 0 && <p className="workspace-menu-head">Recent folders</p>}
          {recent.map((r) => (
            <div key={r.path} className={`workspace-row ${active?.path === r.path ? "is-active" : ""}`}>
              <button
                type="button"
                className="workspace-pick"
                disabled={!r.exists}
                title={r.exists ? r.path : `${r.path} (no longer exists)`}
                onClick={() => {
                  setOpen(false);
                  void ws.activate(r.path);
                }}
              >
                <b>{r.name}</b>
                <small>{r.exists ? r.path : "Missing: moved or deleted"}</small>
              </button>
              <button type="button" className="workspace-forget" aria-label={`Remove ${r.name} from the list`} title="Remove from the list (the folder is not touched)" onClick={() => void ws.forget(r.path)}>
                ×
              </button>
            </div>
          ))}
          {runs.length > 0 && <p className="workspace-menu-head">Run sandboxes</p>}
          {runs.slice(0, 6).map((r) => (
            <div key={r.path} className={`workspace-row ${active?.path === r.path ? "is-active" : ""}`}>
              <button
                type="button"
                className="workspace-pick"
                title={r.path}
                onClick={() => {
                  setOpen(false);
                  void ws.activate(r.path);
                }}
              >
                <b>{runSandboxLabel(r.name)}</b>
                <small>What a REX run made</small>
              </button>
            </div>
          ))}
          <button
            type="button"
            className="workspace-open"
            onClick={() => {
              setOpen(false);
              void ws.pick();
            }}
          >
            Open folder…
          </button>
        </div>
      )}
    </div>
  );
}

// The current branch and whether there is anything to commit, on every tab.
// A click goes to the Git tab. Hidden until a folder is open.
export function GitStatusChip({ onOpen }: { onOpen: () => void }) {
  const ws = useWorkspace();
  if (!ws.desktop || !ws.active) return null;
  const chip = gitChip(ws.git, ws.gitError);
  return (
    <button type="button" className={`git-chip git-chip--${chip.tone}`} onClick={onOpen} title={chip.title} aria-label={`Git status: ${chip.label}. ${chip.title}. Open the Git tab`}>
      <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><circle cx="4" cy="3.5" r="1.7" fill="none" stroke="currentColor" strokeWidth="1.4" /><circle cx="4" cy="12.5" r="1.7" fill="none" stroke="currentColor" strokeWidth="1.4" /><circle cx="12" cy="6.5" r="1.7" fill="none" stroke="currentColor" strokeWidth="1.4" /><path d="M4 5.2v5.6M12 8.2c0 2.4-2 2.6-8 3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" /></svg>
      <span>{chip.label}</span>
    </button>
  );
}
