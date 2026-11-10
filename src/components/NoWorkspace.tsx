import { useWorkspace } from "../data/workspaceContext";
import { runSandboxLabel } from "../data/workspaceLabels";

const COPY = {
  terminal: { title: "Open a folder to start a terminal", body: "The terminal opens in a project folder you choose, with your own shell and settings." },
  editor: { title: "Open a folder to edit its files", body: "Pick a project folder and its files appear here. Saves write straight to the file: it is your edit, so there is no approval step." },
  git: { title: "Open a folder to see its git status", body: "Pick a project folder to see its branch and changes, stage files and commit." },
} as const;

// What the Terminal, Editor and Git tabs show until a folder is open. Folders
// are added only through the system dialog, never by typing a path.
export function NoWorkspace({ feature }: { feature: keyof typeof COPY }) {
  const ws = useWorkspace();
  const copy = COPY[feature];
  const runs = ws.snapshot?.runs ?? [];
  const recent = (ws.snapshot?.recent ?? []).filter((r) => r.exists);
  return (
    <section className="no-workspace" aria-label={copy.title}>
      <span className="no-workspace-icon" aria-hidden="true">
        <svg width="22" height="22" viewBox="0 0 22 22"><path d="M2.5 6.5a2 2 0 0 1 2-2h4l2 2.2h7a2 2 0 0 1 2 2v7.3a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" /></svg>
      </span>
      <h2 className="no-workspace-title">{copy.title}</h2>
      <p className="no-workspace-body">{copy.body}</p>
      {ws.error && <p className="no-workspace-error" role="alert">{ws.error}</p>}
      <button type="button" className="approval-allow no-workspace-open" onClick={() => void ws.pick()}>
        Open folder…
      </button>
      {recent.length > 0 && (
        <div className="no-workspace-list">
          <p className="eyebrow">Recent folders</p>
          {recent.slice(0, 5).map((r) => (
            <button key={r.path} type="button" className="no-workspace-item" title={r.path} onClick={() => void ws.activate(r.path)}>
              <b>{r.name}</b>
              <small>{r.path}</small>
            </button>
          ))}
        </div>
      )}
      {runs.length > 0 && (
        <div className="no-workspace-list">
          <p className="eyebrow">Or look at what a recent run made</p>
          {runs.slice(0, 4).map((r) => (
            <button key={r.path} type="button" className="no-workspace-item" title={r.path} onClick={() => void ws.activate(r.path)}>
              <b>{runSandboxLabel(r.name)}</b>
              <small>REX run sandbox</small>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
