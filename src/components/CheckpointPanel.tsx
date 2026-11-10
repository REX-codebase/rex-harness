import { useEffect, useState } from "react";
import {
  checkpointCreate,
  checkpointList,
  checkpointRestore,
  type Checkpoint,
} from "../data/checkpoint";
import { errorText } from "../data/runLifecycle";
import { useWorkspace } from "../data/workspaceContext";

// Checkpoint panel: list snapshots of the open folder and restore with one click.
// Restore auto-snapshots the current state first, so it's reversible. A
// snapshot covers the project's files, not regenerable folders such as
// node_modules, target or dist, which are neither copied nor deleted.
export function CheckpointPanel() {
  const ws = useWorkspace();
  const workspace = ws.active?.path ?? null;
  const [checkpoints, setCheckpoints] = useState<Checkpoint[]>([]);
  const [label, setLabel] = useState("");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = async (folder: string) => {
    setLoading(true);
    setError(null);
    try {
      setCheckpoints(await checkpointList(folder));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setCheckpoints([]);
    setNotice(null);
    setError(null);
    if (workspace) void refresh(workspace);
  }, [workspace]);

  const create = async () => {
    if (!workspace || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const cp = await checkpointCreate(
        workspace,
        label.trim() || `checkpoint ${new Date().toLocaleString()}`,
      );
      setLabel("");
      setNotice(`Saved “${cp.label}” (${cp.file_count} files).`);
      await refresh(workspace);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const restore = async (id: string) => {
    if (!workspace || busy) return;
    if (
      !window.confirm(
        "Restore this checkpoint? Files in this folder are replaced with the snapshot's (node_modules, target and similar folders are left alone). The current state is saved first, so you can undo.",
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const backup = await checkpointRestore(workspace, id);
      setNotice(
        `Restored. Previous state saved as “${backup.label}” — you can restore that to undo.`,
      );
      await refresh(workspace);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // The Git tab already explains how to open a folder; nothing to add here.
  if (!workspace) return null;

  return (
    <div className="checkpoint-panel" aria-label="Checkpoints">
      <div className="checkpoint-panel-bar">
        <span className="eyebrow">Checkpoints</span>
        <button
          type="button"
          className="reset-button"
          onClick={() => refresh(workspace)}
          disabled={loading}
        >
          Refresh
        </button>
      </div>
      {error && (
        <p className="checkpoint-panel-error" role="alert">
          {error}
        </p>
      )}
      {notice && <p className="checkpoint-panel-notice">{notice}</p>}
      <div className="checkpoint-panel-create">
        <input
          type="text"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="Label for this snapshot…"
          aria-label="Checkpoint label"
          disabled={busy}
        />
        <button
          type="button"
          className="approval-allow"
          onClick={create}
          disabled={busy}
        >
          {busy ? "Working…" : "Snapshot now"}
        </button>
      </div>
      {loading ? (
        <p>Loading…</p>
      ) : checkpoints.length === 0 ? (
        <p className="checkpoint-panel-empty">
          No checkpoints yet. Snapshot before a risky change.
        </p>
      ) : (
        <ul className="checkpoint-panel-list">
          {checkpoints.map((cp) => (
            <li key={cp.id}>
              <div>
                <b>{cp.label}</b>
                <small>
                  {new Date(cp.created_at_ms).toLocaleString()} · {cp.file_count}{" "}
                  files
                </small>
              </div>
              <button
                type="button"
                className="reset-button"
                onClick={() => restore(cp.id)}
                disabled={busy}
              >
                Restore
              </button>
            </li>
          ))}
        </ul>
      )}
      <p className="settings-note">
        Restoring auto-snapshots the current folder first. Nothing is lost —
        you can restore the backup to undo. A snapshot covers your project's
        files, not node_modules, target or dist, and a folder over 20,000 files
        or 256 MB is too big to snapshot.
      </p>
    </div>
  );
}
