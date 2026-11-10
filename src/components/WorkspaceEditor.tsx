import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import { workspaceCreateFile, workspaceReadFile, workspaceTree, workspaceWriteFile } from "../data/workspace";
import { errorText } from "../data/runLifecycle";
import { languageFor } from "../data/language";
import { useWorkspace } from "../data/workspaceContext";
import { useMonacoReady } from "../lib/monaco";
import { NoWorkspace } from "./NoWorkspace";

// The folder's files and a Monaco editor. Saves write straight to the file:
// this is the person's own edit, so there is no approval step. Kept mounted once
// shown (see TerminalView), so an open file and its unsaved edits survive a trip
// to another tab.
export function WorkspaceEditor({ active }: { active: boolean }) {
  const ws = useWorkspace();
  const workspace = ws.active?.path ?? null;
  if (!workspace) return active ? <NoWorkspace feature="editor" /> : null;
  return <EditorSession key={workspace} workspace={workspace} folderName={ws.active?.name ?? ""} active={active} />;
}

function EditorSession({ workspace, folderName, active }: { workspace: string; folderName: string; active: boolean }) {
  const ws = useWorkspace();
  const reportUnsaved = ws.reportUnsaved;
  const monacoReady = useMonacoReady();
  const [files, setFiles] = useState<string[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [filter, setFilter] = useState("");
  const [listing, setListing] = useState<"loading" | "ready" | "failed">("loading");
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [opening, setOpening] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const dirty = selected !== null && content !== saved;
  const rootRef = useRef<HTMLDivElement>(null);

  const loadFiles = useCallback(async () => {
    setListing("loading");
    try {
      const found = await workspaceTree(workspace);
      setFiles(found.files);
      setTruncated(found.truncated);
      setListing("ready");
    } catch (e) {
      setError(errorText(e));
      setListing("failed");
    }
  }, [workspace]);

  useEffect(() => {
    void loadFiles();
  }, [loadFiles]);

  // Switching folders, or closing the window, with unsaved edits asks first.
  useEffect(() => {
    reportUnsaved("editor", dirty);
    return () => reportUnsaved("editor", false);
  }, [dirty, reportUnsaved]);
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  const openFile = async (path: string) => {
    if (path === selected) return;
    if (dirty && !window.confirm(`Discard your unsaved changes to ${selected}?`)) return;
    setOpening(true);
    setError(null);
    setNotice(null);
    try {
      const file = await workspaceReadFile(workspace, path);
      setSelected(path);
      setContent(file.content);
      setSaved(file.content);
    } catch (e) {
      // keep the previous file open; say why this one could not be
      setError(errorText(e));
    } finally {
      setOpening(false);
    }
  };

  const save = useCallback(async () => {
    if (!selected || content === saved || saving) return;
    setSaving(true);
    setError(null);
    try {
      await workspaceWriteFile(workspace, selected, content);
      setSaved(content);
      setNotice(`Saved ${selected}`);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  }, [workspace, selected, content, saved, saving]);

  const createFile = async () => {
    const name = newName.trim();
    if (!name) return;
    if (dirty && !window.confirm(`Discard your unsaved changes to ${selected}?`)) return;
    setError(null);
    try {
      await workspaceCreateFile(workspace, name);
      setCreating(false);
      setNewName("");
      await loadFiles();
      const normalized = name.replace(/\\/g, "/").replace(/^\.?\/+/, "");
      const file = await workspaceReadFile(workspace, normalized);
      setSelected(normalized);
      setContent(file.content);
      setSaved(file.content);
      setNotice(`Created ${normalized}`);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? files.filter((f) => f.toLowerCase().includes(q)) : files;
  }, [files, filter]);

  return (
    <div
      ref={rootRef}
      className="workspace-editor"
      aria-label="Workspace editor"
      hidden={!active}
      onKeyDown={(e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
          e.preventDefault();
          void save();
        }
      }}
    >
      <div className="workspace-editor-bar">
        <span className="eyebrow">Editor</span>
        <span className="workspace-editor-folder" title={workspace}>
          {folderName}
        </span>
        <span className="workspace-editor-spacer" />
        {selected && (
          <span className="workspace-editor-file" title={selected}>
            {selected}
          </span>
        )}
        <button type="button" onClick={() => void save()} disabled={!dirty || saving} className="reset-button" title="Save (Ctrl+S)">
          {saving ? "Saving…" : dirty ? "Save ●" : "Saved"}
        </button>
      </div>

      {error && (
        <p className="workspace-editor-error" role="alert">
          {error}
        </p>
      )}
      {notice && !error && (
        <p className="workspace-editor-notice" role="status">
          {notice}
        </p>
      )}

      <div className="workspace-editor-layout">
        <aside className="workspace-files" aria-label="Files">
          <div className="workspace-files-tools">
            <input type="search" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter files" aria-label="Filter files" />
            <button type="button" className="workspace-files-new" onClick={() => setCreating((v) => !v)} aria-expanded={creating} title="New file">
              +
            </button>
          </div>
          {creating && (
            <form
              className="workspace-files-create"
              onSubmit={(e) => {
                e.preventDefault();
                void createFile();
              }}
            >
              <input autoFocus value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="path/to/new-file.ts" aria-label="New file path" />
              <button type="submit" className="approval-allow" disabled={!newName.trim()}>
                Create
              </button>
            </form>
          )}
          {listing === "loading" && <p className="workspace-files-note" role="status">Reading files…</p>}
          {listing === "ready" && files.length === 0 && <p className="workspace-files-note">This folder has no files yet. Use + to create one.</p>}
          {listing === "ready" && files.length > 0 && shown.length === 0 && <p className="workspace-files-note">No file matches “{filter}”.</p>}
          <ul className="workspace-files-list">
            {shown.map((f) => (
              <li key={f}>
                <button type="button" className={f === selected ? "is-selected" : ""} onClick={() => void openFile(f)} disabled={opening} title={f} aria-current={f === selected ? "true" : undefined}>
                  {f}
                </button>
              </li>
            ))}
          </ul>
          {truncated && <p className="workspace-files-note">Showing the first {files.length} files. Use the filter to find others.</p>}
          <button type="button" className="workspace-files-refresh" onClick={() => void loadFiles()}>
            Refresh list
          </button>
        </aside>

        <div className="workspace-editor-body">
          {selected ? (
            monacoReady ? (
              <Editor
                height="100%"
                theme="vs-dark"
                path={selected}
                language={languageFor(selected)}
                value={content}
                onChange={(v) => setContent(v ?? "")}
                loading={<p className="workspace-editor-empty">Loading editor…</p>}
                options={{ minimap: { enabled: false }, scrollBeyondLastLine: false, fontSize: 13, automaticLayout: true, tabSize: 2 }}
              />
            ) : (
              <p className="workspace-editor-empty" role="status">Loading editor…</p>
            )
          ) : (
            <p className="workspace-editor-empty">Choose a file on the left. Your saves write straight to it: this is your edit, so there is no approval step.</p>
          )}
        </div>
      </div>
    </div>
  );
}
