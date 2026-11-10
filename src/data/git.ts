// Git bindings and the pure helpers the Git tab and the status chip share.
// Everything here is the person acting on their own repository with their own
// git: the commit message is theirs and nothing goes through the model gate.

import { invoke } from "@tauri-apps/api/core";

export interface GitChange {
  path: string;
  orig_path: string | null;
  /** Staged state: "." unchanged, else M A D R C T. */
  index: string;
  /** Working-tree state: "." unchanged, else M D T. */
  worktree: string;
  untracked: boolean;
  conflicted: boolean;
}

export interface GitOverview {
  git_available: boolean;
  is_repo: boolean;
  root: string | null;
  branch: string | null;
  detached: boolean;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
  files: GitChange[];
  truncated: boolean;
  message: string | null;
}

export interface CommitInfo {
  hash: string;
  subject: string;
}
export interface CommitSummary {
  hash: string;
  subject: string;
  author: string;
  when: string;
}

export const gitOverview = (workspace: string) => invoke<GitOverview>("git_overview", { workspace });
export const gitStage = (workspace: string, paths: string[]) => invoke<void>("git_stage", { workspace, paths });
export const gitStageAll = (workspace: string) => invoke<void>("git_stage_all", { workspace });
export const gitUnstage = (workspace: string, paths: string[]) => invoke<void>("git_unstage", { workspace, paths });
export const gitDiff = (workspace: string, path: string, staged: boolean) =>
  invoke<string>("git_diff", { workspace, path, staged });
export const gitCommit = (workspace: string, message: string) => invoke<CommitInfo>("git_commit", { workspace, message });
export const gitInit = (workspace: string) => invoke<string>("git_init", { workspace });
export const gitLog = (workspace: string, limit = 20) => invoke<CommitSummary[]>("git_log", { workspace, limit });

export interface GroupedChanges {
  conflicted: GitChange[];
  staged: GitChange[];
  unstaged: GitChange[];
  untracked: GitChange[];
}

/** The change list as the tab shows it: a file edited after staging is in both. */
export function groupChanges(files: GitChange[]): GroupedChanges {
  const out: GroupedChanges = { conflicted: [], staged: [], unstaged: [], untracked: [] };
  for (const f of files) {
    if (f.conflicted) out.conflicted.push(f);
    else if (f.untracked) out.untracked.push(f);
    else {
      if (f.index !== ".") out.staged.push(f);
      if (f.worktree !== ".") out.unstaged.push(f);
    }
  }
  return out;
}

const LETTERS: Record<string, string> = {
  M: "Modified",
  A: "Added",
  D: "Deleted",
  R: "Renamed",
  C: "Copied",
  T: "Type changed",
  U: "Unmerged",
};
export const changeLetter = (state: string): string => (state === "." ? "" : state);
export const changeWord = (state: string): string => LETTERS[state] ?? "Changed";

export type GitChipTone = "none" | "clean" | "dirty" | "alert" | "idle";
export interface GitChip {
  tone: GitChipTone;
  /** The short text in the chip. */
  label: string;
  /** The longer explanation for the tooltip and screen readers. */
  title: string;
}

/** "↑2 ↓1", or nothing when level with the upstream. */
export function syncLabel(o: Pick<GitOverview, "ahead" | "behind">): string {
  return [o.ahead > 0 ? `↑${o.ahead}` : "", o.behind > 0 ? `↓${o.behind}` : ""].filter(Boolean).join(" ");
}

/** The branch as a person reads it: its name, or "detached at abc1234". */
export function branchName(o: Pick<GitOverview, "branch" | "detached" | "head">): string {
  if (o.branch) return o.branch;
  if (o.detached) return o.head ? `detached at ${o.head}` : "detached HEAD";
  return "no commits yet";
}

/** What the status chip says for the folder's current git state. */
export function gitChip(overview: GitOverview | null, error: string | null = null): GitChip {
  if (error) return { tone: "alert", label: "Git unavailable", title: error };
  if (!overview) return { tone: "idle", label: "Git…", title: "Reading git status" };
  if (!overview.git_available) {
    return { tone: "none", label: "Git not installed", title: overview.message ?? "Install git to see the status of this folder." };
  }
  if (overview.message && !overview.is_repo) return { tone: "alert", label: "Git unavailable", title: overview.message };
  if (!overview.is_repo) {
    return { tone: "none", label: "Not a git repo", title: "This folder is not a git repository. Open the Git tab to start one." };
  }
  const name = branchName(overview);
  const sync = syncLabel(overview);
  const base = sync ? `${name} ${sync}` : name;
  const changes = overview.staged + overview.unstaged + overview.untracked + overview.conflicted;
  if (overview.message) return { tone: "alert", label: base, title: overview.message };
  if (overview.conflicted > 0) {
    return {
      tone: "alert",
      label: `${base} · ${overview.conflicted} conflict${overview.conflicted === 1 ? "" : "s"}`,
      title: `${overview.conflicted} file${overview.conflicted === 1 ? "" : "s"} with merge conflicts to resolve`,
    };
  }
  if (changes === 0) return { tone: "clean", label: `${base} · clean`, title: `${name}: nothing to commit` + (sync ? ` (${describeSync(overview)})` : "") };
  const parts = [
    overview.staged ? `${overview.staged} staged` : "",
    overview.unstaged ? `${overview.unstaged} modified` : "",
    overview.untracked ? `${overview.untracked} untracked` : "",
  ].filter(Boolean);
  return {
    tone: "dirty",
    label: `${base} · ${changes} change${changes === 1 ? "" : "s"}`,
    title: `${name}: ${parts.join(", ")}` + (sync ? ` (${describeSync(overview)})` : ""),
  };
}

function describeSync(o: Pick<GitOverview, "ahead" | "behind" | "upstream">): string {
  const where = o.upstream ?? "its upstream";
  return [o.ahead > 0 ? `${o.ahead} ahead of ${where}` : "", o.behind > 0 ? `${o.behind} behind ${where}` : ""]
    .filter(Boolean)
    .join(", ");
}
