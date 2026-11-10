// The one place the open folder and its git state live, so the status chip, the
// folder menu and the Terminal, Editor and Git tabs always agree about both.
//
// Git is read on a timer while the window is in view and again whenever it
// regains focus (the person has usually just been in another program), and
// after anything this app does to the repository. Reads never overlap, and an
// answer for a folder that is no longer open is dropped.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { isDesktopRuntime } from "../components/DesktopOnly";
import { errorText } from "./runLifecycle";
import * as workspaceApi from "./workspace";
import * as gitApi from "./git";
import type { WorkspaceInfo, WorkspaceSnapshot } from "./workspace";
import type { GitOverview } from "./git";

export interface WorkspaceApi {
  state: () => Promise<WorkspaceSnapshot>;
  pick: () => Promise<WorkspaceSnapshot | null>;
  activate: (path: string) => Promise<WorkspaceSnapshot>;
  forget: (path: string) => Promise<WorkspaceSnapshot>;
  gitOverview: (workspace: string) => Promise<GitOverview>;
}

const realApi: WorkspaceApi = {
  state: workspaceApi.workspaceState,
  pick: workspaceApi.workspacePick,
  activate: workspaceApi.workspaceActivate,
  forget: workspaceApi.workspaceForget,
  gitOverview: gitApi.gitOverview,
};

export interface WorkspaceValue {
  /** The desktop runtime is present, so folders and git are available. */
  desktop: boolean;
  snapshot: WorkspaceSnapshot | null;
  active: WorkspaceInfo | null;
  /** A folder-level problem (the list could not be read, a dialog failed). */
  error: string | null;
  pick: () => Promise<void>;
  activate: (path: string) => Promise<void>;
  forget: (path: string) => Promise<void>;
  /**
   * A view with unsaved work reports it here, so switching folders asks first
   * instead of silently discarding it.
   */
  reportUnsaved: (key: string, unsaved: boolean) => void;
  git: GitOverview | null;
  /** Git could not be read at all (distinct from "not a repository"). */
  gitError: string | null;
  refreshGit: () => Promise<void>;
}

const Context = createContext<WorkspaceValue | null>(null);

/** How often git is read while the window is in view. */
export const GIT_POLL_MS = 5000;

export function WorkspaceProvider({
  children,
  api = realApi,
  desktop = isDesktopRuntime(),
  pollMs = GIT_POLL_MS,
}: {
  children: ReactNode;
  api?: WorkspaceApi;
  desktop?: boolean;
  pollMs?: number;
}) {
  const [snapshot, setSnapshot] = useState<WorkspaceSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [git, setGit] = useState<GitOverview | null>(null);
  const [gitError, setGitError] = useState<string | null>(null);
  const activePath = snapshot?.active?.path ?? null;
  const reading = useRef(false);
  const generation = useRef(0);
  const unsaved = useRef(new Set<string>());
  const reportUnsaved = useCallback((key: string, dirty: boolean) => {
    if (dirty) unsaved.current.add(key);
    else unsaved.current.delete(key);
  }, []);
  // Leaving the folder with unsaved edits is the person's call, not ours.
  const confirmLeave = useCallback(() => {
    if (unsaved.current.size === 0) return true;
    return window.confirm("You have unsaved changes in the editor. Switch folders and discard them?");
  }, []);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    api
      .state()
      .then((s) => {
        if (alive) setSnapshot(s);
      })
      .catch((e) => {
        if (alive) setError(errorText(e));
      });
    return () => {
      alive = false;
    };
  }, [api, desktop]);

  const refreshGit = useCallback(async () => {
    if (!desktop || !activePath || reading.current) return;
    reading.current = true;
    const gen = generation.current;
    try {
      const overview = await api.gitOverview(activePath);
      if (gen !== generation.current) return;
      setGit(overview);
      setGitError(null);
    } catch (e) {
      if (gen === generation.current) setGitError(errorText(e));
    } finally {
      reading.current = false;
    }
  }, [api, desktop, activePath]);

  // A different folder: forget the old answer and read the new one at once.
  useEffect(() => {
    generation.current += 1;
    reading.current = false;
    setGit(null);
    setGitError(null);
    void refreshGit();
  }, [activePath, refreshGit]);

  useEffect(() => {
    if (!desktop || !activePath) return;
    const tick = () => {
      if (document.visibilityState === "visible") void refreshGit();
    };
    const timer = window.setInterval(tick, pollMs);
    window.addEventListener("focus", tick);
    document.addEventListener("visibilitychange", tick);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", tick);
      document.removeEventListener("visibilitychange", tick);
    };
  }, [desktop, activePath, pollMs, refreshGit]);

  const run = useCallback(
    async (action: () => Promise<WorkspaceSnapshot | null>) => {
      setError(null);
      try {
        const next = await action();
        if (next) setSnapshot(next);
      } catch (e) {
        setError(errorText(e));
      }
    },
    [],
  );

  const value = useMemo<WorkspaceValue>(
    () => ({
      desktop,
      snapshot,
      active: snapshot?.active ?? null,
      error,
      pick: async () => {
        if (confirmLeave()) await run(() => api.pick());
      },
      activate: async (path) => {
        if (path !== snapshot?.active?.path && confirmLeave()) await run(() => api.activate(path));
      },
      forget: (path) => run(() => api.forget(path)),
      reportUnsaved,
      git,
      gitError,
      refreshGit,
    }),
    [api, desktop, snapshot, error, git, gitError, refreshGit, run, confirmLeave, reportUnsaved],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}

/** The open folder and its git state. Outside a provider this is an inert "no folder". */
export function useWorkspace(): WorkspaceValue {
  const value = useContext(Context);
  if (value) return value;
  return {
    desktop: false,
    snapshot: null,
    active: null,
    error: null,
    pick: async () => undefined,
    activate: async () => undefined,
    forget: async () => undefined,
    reportUnsaved: () => undefined,
    git: null,
    gitError: null,
    refreshGit: async () => undefined,
  };
}
