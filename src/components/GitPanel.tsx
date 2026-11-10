import { useCallback, useEffect, useMemo, useState } from "react";
import {
  branchName,
  changeLetter,
  changeWord,
  gitCommit,
  gitDiff,
  gitInit,
  gitLog,
  gitStage,
  gitStageAll,
  gitUnstage,
  groupChanges,
  syncLabel,
  type CommitSummary,
  type GitChange,
} from "../data/git";
import { errorText } from "../data/runLifecycle";
import { useWorkspace } from "../data/workspaceContext";
import { NoWorkspace } from "./NoWorkspace";

type Side = "staged" | "unstaged" | "untracked" | "conflicted";
interface Selection {
  path: string;
  side: Side;
}

const MAX_DIFF_LINES = 4000;

/** One line of a unified diff, classified so it can be coloured. */
export function diffLineKind(line: string): "meta" | "hunk" | "add" | "del" | "ctx" {
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff ") || line.startsWith("index ") || line.startsWith("new file") || line.startsWith("deleted file") || line.startsWith("similarity ") || line.startsWith("rename ")) return "meta";
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  return "ctx";
}

function DiffText({ text }: { text: string }) {
  const all = text.split("\n");
  const shown = all.length > MAX_DIFF_LINES ? all.slice(0, MAX_DIFF_LINES) : all;
  return (
    <pre className="git-diff" aria-label="Diff">
      {shown.map((line, i) => (
        <span key={i} className={`gd gd-${diffLineKind(line)}`}>
          {line === "" ? " " : line}
          {"\n"}
        </span>
      ))}
      {all.length > MAX_DIFF_LINES && <span className="gd gd-meta">{`… ${all.length - MAX_DIFF_LINES} more lines not shown\n`}</span>}
    </pre>
  );
}

function stateFor(change: GitChange, side: Side): string {
  if (side === "staged") return change.index;
  if (side === "unstaged") return change.worktree;
  if (side === "untracked") return "?";
  return "!";
}

function FileRow({
  change,
  side,
  selected,
  busy,
  actionLabel,
  onOpen,
  onAction,
}: {
  change: GitChange;
  side: Side;
  selected: boolean;
  busy: boolean;
  actionLabel?: string;
  onOpen: () => void;
  onAction?: () => void;
}) {
  const state = stateFor(change, side);
  const letter = side === "untracked" ? "U" : side === "conflicted" ? "!" : changeLetter(state);
  const word = side === "untracked" ? "Untracked" : side === "conflicted" ? "Conflict" : changeWord(state);
  const shown = change.orig_path ? `${change.orig_path} → ${change.path}` : change.path;
  return (
    <li className={`git-row ${selected ? "is-selected" : ""}`}>
      <button type="button" className="git-row-main" onClick={onOpen} aria-pressed={selected} title={shown}>
        <span className={`git-badge git-badge--${letter === "U" ? "A" : letter}`} title={word} aria-label={word}>
          {letter}
        </span>
        <span className="git-path">{shown}</span>
      </button>
      {onAction && actionLabel && (
        <button type="button" className="git-row-action" disabled={busy} aria-label={`${actionLabel} ${change.path}`} onClick={onAction}>
          {actionLabel}
        </button>
      )}
    </li>
  );
}

// The Git tab: the branch, what changed, and the actions to commit it. It is
// the person's own repository and their own git, so the commit is their action
// with their message and nothing goes through the model approval gate.
export function GitPanel() {
  const ws = useWorkspace();
  const { active, git, gitError, refreshGit } = ws;
  const workspace = active?.path ?? null;

  const [selected, setSelected] = useState<Selection | null>(null);
  const [diff, setDiff] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [log, setLog] = useState<CommitSummary[]>([]);

  const groups = useMemo(() => groupChanges(git?.files ?? []), [git?.files]);

  // A different folder starts from a clean slate.
  useEffect(() => {
    setSelected(null);
    setDiff(null);
    setMessage("");
    setError(null);
    setNotice(null);
    setLog([]);
  }, [workspace]);

  // Recent commits follow HEAD, so they update after a commit made here or in a terminal.
  const head = git?.head ?? null;
  const isRepo = git?.is_repo ?? false;
  useEffect(() => {
    if (!workspace || !isRepo || !head) {
      setLog([]);
      return;
    }
    let alive = true;
    gitLog(workspace, 8)
      .then((rows) => {
        if (alive) setLog(rows);
      })
      .catch(() => {
        if (alive) setLog([]);
      });
    return () => {
      alive = false;
    };
  }, [workspace, isRepo, head]);

  const loadDiff = useCallback(
    async (sel: Selection) => {
      if (!workspace) return;
      try {
        const text = await gitDiff(workspace, sel.path, sel.side === "staged");
        setDiff(text);
      } catch (e) {
        setDiff(null);
        setError(errorText(e));
      }
    },
    [workspace],
  );

  // Keep the open diff in step with the repository (the person may stage in a terminal).
  useEffect(() => {
    if (!selected) return;
    const still = groups[selected.side].some((c) => c.path === selected.path);
    if (!still) {
      setSelected(null);
      setDiff(null);
      return;
    }
    void loadDiff(selected);
    // `groups` changes identity on every status read; the content is what matters
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [git?.files]);

  const open = (path: string, side: Side) => {
    const sel = { path, side };
    setSelected(sel);
    setDiff(null);
    void loadDiff(sel);
  };

  const act = async (label: string, action: () => Promise<unknown>) => {
    if (!workspace || busy) return;
    setBusy(label);
    setError(null);
    setNotice(null);
    try {
      await action();
      await refreshGit();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const commit = () =>
    act("commit", async () => {
      const done = await gitCommit(workspace!, message);
      setNotice(`Committed ${done.hash} · ${done.subject}`);
      setMessage("");
      setSelected(null);
      setDiff(null);
    });

  if (!workspace) return <NoWorkspace feature="git" />;

  const header = (
    <div className="git-panel-bar">
      <div className="git-panel-title">
        <span className="eyebrow">Git</span>
        <b>{active?.name}</b>
      </div>
      <button type="button" className="reset-button" onClick={() => void refreshGit()} disabled={busy !== null}>
        Refresh
      </button>
    </div>
  );

  if (gitError) {
    return (
      <div className="git-panel" aria-label="Git">
        {header}
        <p className="git-panel-error" role="alert">
          Could not read git status: {gitError}
        </p>
      </div>
    );
  }

  if (!git) {
    return (
      <div className="git-panel" aria-label="Git">
        {header}
        <p className="git-panel-empty" role="status">
          Reading git status…
        </p>
      </div>
    );
  }

  if (!git.git_available) {
    return (
      <div className="git-panel" aria-label="Git">
        {header}
        <p className="git-panel-empty">
          Git is not installed on this computer. Install it from <code>https://git-scm.com</code>, then reopen REX.
        </p>
      </div>
    );
  }

  if (!git.is_repo) {
    return (
      <div className="git-panel" aria-label="Git">
        {header}
        {git.message ? (
          <p className="git-panel-error" role="alert">
            {git.message}
          </p>
        ) : (
          <div className="git-init">
            <h2>This folder is not a git repository</h2>
            <p>Start one to track changes, see what an agent run changed, and commit your work.</p>
            <button type="button" className="approval-allow" disabled={busy !== null} onClick={() => void act("init", () => gitInit(workspace))}>
              {busy === "init" ? "Starting…" : "Initialize repository"}
            </button>
          </div>
        )}
        {error && (
          <p className="git-panel-error" role="alert">
            {error}
          </p>
        )}
      </div>
    );
  }

  const stagedCount = groups.staged.length;
  const hasChanges = git.staged + git.unstaged + git.untracked + git.conflicted > 0;
  const sync = syncLabel(git);
  const canCommit = stagedCount > 0 && message.trim().length > 0 && busy === null && git.conflicted === 0;
  const subjectLong = message.split("\n")[0].length > 72;

  const section = (title: string, side: Side, rows: GitChange[], bulk?: { label: string; run: () => void }, rowAction?: { label: string; run: (c: GitChange) => void }) =>
    rows.length === 0 ? null : (
      <section className="git-section" aria-label={title}>
        <div className="git-section-head">
          <h3>
            {title} <span className="git-count">{rows.length}</span>
          </h3>
          {bulk && (
            <button type="button" className="git-bulk" disabled={busy !== null} onClick={bulk.run}>
              {bulk.label}
            </button>
          )}
        </div>
        <ul className="git-files">
          {rows.map((c) => (
            <FileRow
              key={`${side}:${c.path}`}
              change={c}
              side={side}
              selected={selected?.side === side && selected.path === c.path}
              busy={busy !== null}
              actionLabel={rowAction?.label}
              onOpen={() => open(c.path, side)}
              onAction={rowAction ? () => rowAction.run(c) : undefined}
            />
          ))}
        </ul>
      </section>
    );

  return (
    <div className="git-panel" aria-label="Git">
      {header}

      <div className="git-branch" role="status">
        <span className="git-branch-name">{branchName(git)}</span>
        {git.head && !git.detached && <code className="git-head">{git.head}</code>}
        {sync && (
          <span className="git-sync" title={git.upstream ? `Compared with ${git.upstream}` : undefined}>
            {sync}
          </span>
        )}
        {git.upstream ? <small className="git-upstream">tracking {git.upstream}</small> : git.branch ? <small className="git-upstream">no upstream</small> : null}
        {git.root && git.root !== active?.path && <small className="git-upstream" title={git.root}>repo root: {git.root}</small>}
      </div>

      {git.message && (
        <p className="git-panel-error" role="alert">
          {git.message}
        </p>
      )}
      {error && (
        <p className="git-panel-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="git-panel-result" role="status">
          {notice}
        </p>
      )}

      {!hasChanges ? (
        <p className="git-panel-empty">Working tree clean. Nothing to commit.</p>
      ) : (
        <>
          {section("Conflicts", "conflicted", groups.conflicted)}
          {groups.conflicted.length > 0 && <p className="settings-note">Resolve the conflict markers in the editor, then stage the file.</p>}
          {section("Staged", "staged", groups.staged, { label: "Unstage all", run: () => void act("unstage", () => gitUnstage(workspace, groups.staged.map((c) => c.path))) }, { label: "Unstage", run: (c) => void act("unstage", () => gitUnstage(workspace, [c.path])) })}
          {section("Changes", "unstaged", groups.unstaged, { label: "Stage all", run: () => void act("stage", () => gitStageAll(workspace)) }, { label: "Stage", run: (c) => void act("stage", () => gitStage(workspace, [c.path])) })}
          {section("Untracked", "untracked", groups.untracked, undefined, { label: "Stage", run: (c) => void act("stage", () => gitStage(workspace, [c.path])) })}
          {git.truncated && <p className="settings-note">More files changed than are listed here. The counts above are exact.</p>}
        </>
      )}

      {selected && (
        <div className="git-panel-diff">
          <p className="git-path">
            {selected.path} <small>{selected.side === "staged" ? "staged" : selected.side === "untracked" ? "new file" : "not staged"}</small>
          </p>
          {diff === null ? <p className="git-panel-empty" role="status">Loading diff…</p> : <DiffText text={diff || "(no differences)"} />}
        </div>
      )}

      <div className="git-panel-commit">
        <label className="git-commit-label" htmlFor="git-commit-message">
          Commit message
        </label>
        <textarea
          id="git-commit-message"
          rows={3}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && canCommit) {
              e.preventDefault();
              void commit();
            }
          }}
          placeholder={stagedCount > 0 ? "Summarize the change" : "Stage some files to commit them"}
          aria-label="Commit message"
          disabled={busy !== null}
        />
        <div className="git-commit-row">
          <small className={subjectLong ? "git-hint is-warn" : "git-hint"}>
            {subjectLong ? "The first line is long; under 72 characters reads best." : stagedCount === 0 && hasChanges ? "Nothing is staged yet." : "Ctrl+Enter commits."}
          </small>
          <button type="button" className="approval-allow" onClick={() => void commit()} disabled={!canCommit}>
            {busy === "commit" ? "Committing…" : stagedCount > 0 ? `Commit ${stagedCount} file${stagedCount === 1 ? "" : "s"}` : "Commit"}
          </button>
        </div>
      </div>

      {log.length > 0 && (
        <details className="git-log">
          <summary>Recent commits</summary>
          <ol>
            {log.map((c) => (
              <li key={c.hash}>
                <code>{c.hash}</code>
                <span className="git-log-subject">{c.subject}</span>
                <small>
                  {c.author} · {c.when}
                </small>
              </li>
            ))}
          </ol>
        </details>
      )}
    </div>
  );
}
