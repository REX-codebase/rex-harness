// Workspace bindings: the folder the Terminal, Editor and Git tabs work in, and
// the editor's file access. The backend decides what a path may be: a folder
// only counts once the person has opened it through the native dialog, and the
// webview cannot add one by naming a path.

import { invoke } from "@tauri-apps/api/core";

export interface FileDiff {
  path: string;
  original: string;
  modified: string;
}

/** The before/after of a file write waiting for approval. */
export async function toolCallDiff(callId: string): Promise<FileDiff | null> {
  return invoke<FileDiff | null>("tool_call_diff", { callId });
}

export interface WorkspaceInfo {
  path: string;
  name: string;
  /** False when the folder was moved or deleted since it was opened. */
  exists: boolean;
}

export interface WorkspaceSnapshot {
  active: WorkspaceInfo | null;
  /** Folders the person opened, most recent first. */
  recent: WorkspaceInfo[];
  /** REX's own run sandboxes, newest first. */
  runs: WorkspaceInfo[];
}

export const workspaceState = () => invoke<WorkspaceSnapshot>("workspace_state");
/** Opens the system folder dialog; `null` when the person cancels. */
export const workspacePick = () => invoke<WorkspaceSnapshot | null>("workspace_pick");
export const workspaceActivate = (path: string) => invoke<WorkspaceSnapshot>("workspace_activate", { path });
export const workspaceForget = (path: string) => invoke<WorkspaceSnapshot>("workspace_forget", { path });

export interface FileListing {
  /** Paths relative to the folder, with `/`. */
  files: string[];
  truncated: boolean;
}
export interface TextFile {
  content: string;
  size: number;
}

export const workspaceTree = (workspace: string) => invoke<FileListing>("workspace_tree", { workspace });
export const workspaceReadFile = (workspace: string, path: string) =>
  invoke<TextFile>("workspace_read_file", { workspace, path });
export const workspaceWriteFile = (workspace: string, path: string, content: string) =>
  invoke<void>("workspace_write_file", { workspace, path, content });
export const workspaceCreateFile = (workspace: string, path: string) =>
  invoke<void>("workspace_create_file", { workspace, path });

export interface AppInfo {
  version: string;
  data_dir: string;
  runs_root: string;
  platform: string;
  git_available: boolean;
}
export const appInfo = () => invoke<AppInfo>("app_info");
