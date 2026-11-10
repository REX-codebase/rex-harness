// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../data/git", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data/git")>()),
  gitStage: vi.fn(),
  gitStageAll: vi.fn(),
  gitUnstage: vi.fn(),
  gitDiff: vi.fn(),
  gitCommit: vi.fn(),
  gitInit: vi.fn(),
  gitLog: vi.fn(),
}));

import * as git from "../data/git";
import { WorkspaceProvider, type WorkspaceApi } from "../data/workspaceContext";
import { GitPanel } from "./GitPanel";
import type { GitChange, GitOverview } from "../data/git";

const mocked = (fn: unknown) => fn as ReturnType<typeof vi.fn>;
const WS = "/work/app";

const change = (path: string, over: Partial<GitChange> = {}): GitChange => ({
  path, orig_path: null, index: ".", worktree: ".", untracked: false, conflicted: false, ...over,
});
const overview = (over: Partial<GitOverview> = {}): GitOverview => ({
  git_available: true, is_repo: true, root: WS, branch: "main", detached: false, head: "abc1234", upstream: "origin/main",
  ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, files: [], truncated: false, message: null,
  ...over,
});
/** An overview whose counts agree with its files, like the backend's. */
const repo = (files: GitChange[], over: Partial<GitOverview> = {}): GitOverview =>
  overview({
    files,
    staged: files.filter((f) => !f.untracked && !f.conflicted && f.index !== ".").length,
    unstaged: files.filter((f) => !f.untracked && !f.conflicted && f.worktree !== ".").length,
    untracked: files.filter((f) => f.untracked).length,
    conflicted: files.filter((f) => f.conflicted).length,
    ...over,
  });

let current: GitOverview | Error;
const gitOverview = vi.fn(async () => {
  if (current instanceof Error) throw current;
  return current;
});

function renderPanel(active = true) {
  const api: WorkspaceApi = {
    state: async () => ({ active: active ? { path: WS, name: "app", exists: true } : null, recent: [], runs: [] }),
    pick: vi.fn().mockResolvedValue(null),
    activate: vi.fn(),
    forget: vi.fn(),
    gitOverview,
  } as never;
  return render(
    <WorkspaceProvider api={api} desktop pollMs={60_000}>
      <GitPanel />
    </WorkspaceProvider>,
  );
}

beforeEach(() => {
  gitOverview.mockClear();
  mocked(git.gitLog).mockReset().mockResolvedValue([]);
  for (const fn of [git.gitStage, git.gitStageAll, git.gitUnstage, git.gitInit]) mocked(fn).mockReset().mockResolvedValue(undefined);
  mocked(git.gitDiff).mockReset().mockResolvedValue("");
  mocked(git.gitCommit).mockReset();
  current = repo([]);
});
afterEach(cleanup);

describe("states with nothing to show", () => {
  it("asks for a folder first", async () => {
    renderPanel(false);
    expect(await screen.findByText("Open a folder to see its git status")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open folder…" })).toBeTruthy();
  });

  it("says it is reading, then that the tree is clean", async () => {
    renderPanel();
    expect(await screen.findByText(/Working tree clean/)).toBeTruthy();
    expect(screen.getByText("main")).toBeTruthy();
    expect(screen.getByText("tracking origin/main")).toBeTruthy();
  });

  it("explains how to get git when it is missing", async () => {
    current = overview({ git_available: false, is_repo: false });
    renderPanel();
    expect(await screen.findByText(/Git is not installed on this computer/)).toBeTruthy();
  });

  it("shows git's own complaint rather than offering to initialise (a trust or ownership problem)", async () => {
    current = overview({ is_repo: false, message: "fatal: detected dubious ownership in repository" });
    renderPanel();
    expect((await screen.findByRole("alert")).textContent).toContain("dubious ownership");
    expect(screen.queryByRole("button", { name: "Initialize repository" })).toBeNull();
  });

  it("separates 'git could not be read' from 'not a repository'", async () => {
    current = new Error("backend exploded");
    renderPanel();
    expect((await screen.findByRole("alert")).textContent).toContain("backend exploded");
  });

  it("offers to start a repository in a folder that is not one, and reads status again afterwards", async () => {
    current = overview({ is_repo: false, branch: null, head: null });
    // like the backend: once `git init` has run, the folder is a repository
    mocked(git.gitInit).mockImplementation(async () => {
      current = repo([]);
      return "Initialized empty Git repository";
    });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Initialize repository" }));
    await waitFor(() => expect(git.gitInit).toHaveBeenCalledWith(WS));
    expect(await screen.findByText(/Working tree clean/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Initialize repository" })).toBeNull();
  });
});

describe("the change list", () => {
  beforeEach(() => {
    current = repo([
      change("both.txt", { index: "M", worktree: "M" }),
      change("edited.txt", { worktree: "M" }),
      change("new.txt", { untracked: true }),
      change("clash.txt", { index: "U", worktree: "U", conflicted: true }),
      change("renamed.txt", { index: "R", orig_path: "old name.txt" }),
    ]);
  });

  it("groups files into Conflicts, Staged, Changes and Untracked with counts", async () => {
    renderPanel();
    const staged = await screen.findByRole("region", { name: "Staged" });
    expect(within(staged).getAllByRole("listitem")).toHaveLength(2);
    expect(within(staged).getByText("old name.txt → renamed.txt")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Changes" })).getAllByRole("listitem")).toHaveLength(2);
    expect(within(screen.getByRole("region", { name: "Untracked" })).getByText("new.txt")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Conflicts" })).getByText("clash.txt")).toBeTruthy();
    // a file edited after staging is in both lists
    expect(within(staged).getByText("both.txt")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Changes" })).getByText("both.txt")).toBeTruthy();
    expect(screen.getByText(/Resolve the conflict markers/)).toBeTruthy();
  });

  it("stages one file, or all of them, then reads git again", async () => {
    renderPanel();
    await screen.findByRole("region", { name: "Changes" });
    const before = gitOverview.mock.calls.length;
    fireEvent.click(screen.getByRole("button", { name: "Stage edited.txt" }));
    await waitFor(() => expect(git.gitStage).toHaveBeenCalledWith(WS, ["edited.txt"]));
    await waitFor(() => expect(gitOverview.mock.calls.length).toBeGreaterThan(before));
    fireEvent.click(screen.getByRole("button", { name: "Stage new.txt" }));
    await waitFor(() => expect(git.gitStage).toHaveBeenCalledWith(WS, ["new.txt"]));
    fireEvent.click(screen.getByRole("button", { name: "Stage all" }));
    await waitFor(() => expect(git.gitStageAll).toHaveBeenCalledWith(WS));
  });

  it("unstages one file, or every staged file", async () => {
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    fireEvent.click(screen.getByRole("button", { name: "Unstage both.txt" }));
    await waitFor(() => expect(git.gitUnstage).toHaveBeenCalledWith(WS, ["both.txt"]));
    fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));
    await waitFor(() => expect(git.gitUnstage).toHaveBeenLastCalledWith(WS, ["both.txt", "renamed.txt"]));
  });

  it("shows a coloured diff for the file that was clicked, and loads the staged side for staged files", async () => {
    mocked(git.gitDiff).mockResolvedValue("diff --git a/edited.txt b/edited.txt\n@@ -1 +1,2 @@\n context\n+added line\n-removed line");
    renderPanel();
    await screen.findByRole("region", { name: "Changes" });
    fireEvent.click(within(screen.getByRole("region", { name: "Changes" })).getByTitle("edited.txt"));
    await waitFor(() => expect(git.gitDiff).toHaveBeenCalledWith(WS, "edited.txt", false));
    const pre = await screen.findByLabelText("Diff");
    expect(pre.querySelector(".gd-add")?.textContent).toContain("+added line");
    expect(pre.querySelector(".gd-del")?.textContent).toContain("-removed line");
    expect(pre.querySelector(".gd-hunk")?.textContent).toContain("@@");
    expect(screen.getByText("not staged")).toBeTruthy();

    fireEvent.click(within(screen.getByRole("region", { name: "Staged" })).getByTitle("both.txt"));
    await waitFor(() => expect(git.gitDiff).toHaveBeenCalledWith(WS, "both.txt", true));
  });

  it("will not commit while there are conflicts, even with a staged file and a message", async () => {
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "try it" } });
    expect((screen.getByRole("button", { name: /^Commit/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("says when more files changed than are listed", async () => {
    current = repo([change("a.txt", { worktree: "M" })], { truncated: true });
    renderPanel();
    expect(await screen.findByText(/More files changed than are listed/)).toBeTruthy();
  });
});

describe("committing", () => {
  beforeEach(() => {
    current = repo([change("a.txt", { index: "M" }), change("b.txt", { worktree: "M" })]);
  });

  it("needs something staged and a message, and says what is missing", async () => {
    current = repo([change("b.txt", { worktree: "M" })]);
    renderPanel();
    await screen.findByRole("region", { name: "Changes" });
    expect(screen.getByText("Nothing is staged yet.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Commit" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("commits the staged files with the person's message and reports the result", async () => {
    mocked(git.gitCommit).mockResolvedValue({ hash: "9f8e7d6", subject: "fix the thing" });
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    const button = screen.getByRole("button", { name: "Commit 1 file" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    const box = screen.getByLabelText("Commit message") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "fix the thing" } });
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    await waitFor(() => expect(git.gitCommit).toHaveBeenCalledWith(WS, "fix the thing"));
    expect(await screen.findByText("Committed 9f8e7d6 · fix the thing")).toBeTruthy();
    expect(box.value).toBe("");
  });

  it("commits with Ctrl+Enter from the message box", async () => {
    mocked(git.gitCommit).mockResolvedValue({ hash: "1111111", subject: "s" });
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    const box = screen.getByLabelText("Commit message");
    fireEvent.change(box, { target: { value: "via keyboard" } });
    fireEvent.keyDown(box, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(git.gitCommit).toHaveBeenCalledWith(WS, "via keyboard"));
  });

  it("shows git's explanation when the commit is refused, and keeps the message", async () => {
    mocked(git.gitCommit).mockRejectedValue("Git does not know who you are yet. In a terminal run:\n  git config --global user.name \"Your Name\"");
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    const box = screen.getByLabelText("Commit message") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "keep me" } });
    fireEvent.click(screen.getByRole("button", { name: "Commit 1 file" }));
    expect((await screen.findByRole("alert")).textContent).toContain("git config --global user.name");
    expect(box.value).toBe("keep me");
  });

  it("warns, without blocking, when the first line is longer than 72 characters", async () => {
    renderPanel();
    await screen.findByRole("region", { name: "Staged" });
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "x".repeat(80) } });
    expect(screen.getByText(/first line is long/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Commit 1 file" }) as HTMLButtonElement).disabled).toBe(false);
  });
});

describe("recent commits", () => {
  it("lists them under the changes once HEAD exists", async () => {
    mocked(git.gitLog).mockResolvedValue([{ hash: "abc1234", subject: "first commit", author: "Ada", when: "2 hours ago" }]);
    renderPanel();
    expect(await screen.findByText("Recent commits")).toBeTruthy();
    expect(await screen.findByText("first commit")).toBeTruthy();
    expect(screen.getByText("Ada · 2 hours ago")).toBeTruthy();
    expect(git.gitLog).toHaveBeenCalledWith(WS, 8);
  });

  it("is absent for a repository with no commits", async () => {
    current = repo([change("a.txt", { untracked: true })], { head: null, branch: "main" });
    renderPanel();
    await screen.findByRole("region", { name: "Untracked" });
    expect(screen.queryByText("Recent commits")).toBeNull();
    expect(git.gitLog).not.toHaveBeenCalled();
  });
});
