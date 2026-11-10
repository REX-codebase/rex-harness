import { DiffEditor } from "@monaco-editor/react";
import type { FileDiff } from "../data/workspace";
import { languageFor } from "../data/language";
import { useMonacoReady } from "../lib/monaco";

// Monaco diff viewer for the approval flow. Shows the original vs proposed
// content side by side. Read-only — the user approves or denies, they don't
// edit here. Uses the Monaco bundled with the app, so it works offline.
export function DiffViewer({ diff }: { diff: FileDiff }) {
  const ready = useMonacoReady();
  return (
    <div className="diff-viewer" aria-label={`Diff for ${diff.path}`}>
      <p className="diff-viewer-path">{diff.path}</p>
      {ready ? (
        <DiffEditor
          height="320px"
          theme="vs-dark"
          language={languageFor(diff.path)}
          original={diff.original}
          modified={diff.modified}
          options={{
            readOnly: true,
            renderSideBySide: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
          }}
        />
      ) : (
        <p className="diff-viewer-loading" role="status">Loading diff…</p>
      )}
    </div>
  );
}
