// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkspaceProvider, type WorkspaceApi } from "../data/workspaceContext";
import { GitStatusChip, WorkspaceChip } from "./WorkspaceChip";
import type { WorkspaceSnapshot } from "../data/workspace";
import type { GitOverview } from "../data/git";

afterEach(cleanup);

const folder = (name: string, exists = true) => ({ path: `/work/${name}`, name, exists });
const overview = (over: Partial<GitOverview> = {}): GitOverview => ({
  git_available: true, is_repo: true, root: "/work/app", branch: "main", detached: false, head: "abc1234", upstream: "origin/main",
  ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, files: [], truncated: false, message: null,
  ...over,
});

function setup(snapshot: WorkspaceSnapshot, git: GitOverview = overview(), extra: Partial<WorkspaceApi> = {}) {
  const api = {
    state: vi.fn().mockResolvedValue(snapshot),
    pick: vi.fn().mockResolvedValue(null),
    activate: vi.fn().mockImplementation(async (path: string) => ({ ...snapshot, active: { path, name: path.split("/").pop()!, exists: true } })),
    forget: vi.fn().mockImplementation(async (path: string) => ({ ...snapshot, recent: snapshot.recent.filter((r) => r.path !== path) })),
    gitOverview: vi.fn().mockResolvedValue(git),
    ...extra,
  } as unknown as WorkspaceApi & Record<string, ReturnType<typeof vi.fn>>;
  const onOpen = vi.fn();
  render(
    <WorkspaceProvider api={api} desktop pollMs={60_000}>
      <WorkspaceChip />
      <GitStatusChip onOpen={onOpen} />
    </WorkspaceProvider>,
  );
  return { api, onOpen };
}

describe("the folder chip", () => {
  it("is not shown outside the desktop app", () => {
    render(
      <WorkspaceProvider desktop={false}>
        <WorkspaceChip />
        <GitStatusChip onOpen={() => undefined} />
      </WorkspaceProvider>,
    );
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("invites the person to open a folder when there is none, and lists nothing else", async () => {
    const { api } = setup({ active: null, recent: [], runs: [] });
    const button = await screen.findByRole("button", { name: "Open a folder" });
    fireEvent.click(button);
    expect(screen.getByRole("dialog", { name: "Folders" })).toBeTruthy();
    expect(screen.queryByText("Recent folders")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open folder…" }));
    await waitFor(() => expect(api.pick).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("names the active folder and lets the person switch, removing the menu afterwards", async () => {
    const snapshot = { active: folder("app"), recent: [folder("app"), folder("site")], runs: [] };
    const { api } = setup(snapshot);
    const button = await screen.findByRole("button", { name: "Folder: app. Change folder" });
    expect(button.getAttribute("title")).toBe("/work/app");
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("button", { name: /^site/ }));
    await waitFor(() => expect(api.activate).toHaveBeenCalledWith("/work/site"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByRole("button", { name: "Folder: site. Change folder" })).toBeTruthy();
  });

  it("removes a folder from the list without touching it, and disables one that has gone missing", async () => {
    const snapshot = { active: folder("app"), recent: [folder("app"), folder("old", false)], runs: [] };
    const { api } = setup(snapshot);
    fireEvent.click(await screen.findByRole("button", { name: /Change folder/ }));
    const missing = screen.getByRole("button", { name: /^old/ }) as HTMLButtonElement;
    expect(missing.disabled).toBe(true);
    expect(screen.getByText("Missing: moved or deleted")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remove old from the list" }));
    await waitFor(() => expect(api.forget).toHaveBeenCalledWith("/work/old"));
  });

  it("lists REX's run sandboxes by when they were made", async () => {
    const ms = Date.UTC(2026, 10, 10, 9, 30);
    const run = { path: `/runs/task-ui-${ms.toString(16)}`, name: `task-ui-${ms.toString(16)}`, exists: true };
    const { api } = setup({ active: folder("app"), recent: [folder("app")], runs: [run] });
    fireEvent.click(await screen.findByRole("button", { name: /Change folder/ }));
    expect(screen.getByText("Run sandboxes")).toBeTruthy();
    const item = screen.getByRole("button", { name: /Run · / });
    expect(screen.queryByText(run.name)).toBeNull();
    fireEvent.click(item);
    await waitFor(() => expect(api.activate).toHaveBeenCalledWith(run.path));
  });

  it("closes on Escape and on a click outside", async () => {
    setup({ active: folder("app"), recent: [folder("app")], runs: [] });
    fireEvent.click(await screen.findByRole("button", { name: /Change folder/ }));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Change folder/ }));
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows why the folder list failed instead of an empty menu", async () => {
    const snapshot = { active: null, recent: [], runs: [] };
    const { api } = setup(snapshot, overview(), { pick: vi.fn().mockRejectedValue("the dialog could not open") });
    fireEvent.click(await screen.findByRole("button", { name: "Open a folder" }));
    fireEvent.click(screen.getByRole("button", { name: "Open folder…" }));
    await waitFor(() => expect(api.pick).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Open a folder" }));
    expect((await screen.findByRole("alert")).textContent).toBe("the dialog could not open");
  });
});

describe("the git status chip", () => {
  const snapshot = { active: folder("app"), recent: [folder("app")], runs: [] };

  it("is hidden until a folder is open", async () => {
    setup({ active: null, recent: [], runs: [] });
    await screen.findByRole("button", { name: "Open a folder" });
    expect(screen.queryByRole("button", { name: /Git status/ })).toBeNull();
  });

  it("shows the branch and the number of changes, and opens the Git tab on click", async () => {
    const { onOpen } = setup(snapshot, overview({ unstaged: 2, untracked: 1, ahead: 1, upstream: "origin/main" }));
    const chip = await screen.findByRole("button", { name: /Git status: main ↑1 · 3 changes/ });
    expect(chip.className).toContain("git-chip--dirty");
    expect(chip.getAttribute("title")).toContain("2 modified, 1 untracked");
    fireEvent.click(chip);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("says clean when there is nothing to commit", async () => {
    setup(snapshot, overview());
    const chip = await screen.findByRole("button", { name: /Git status: main · clean/ });
    expect(chip.className).toContain("git-chip--clean");
  });

  it("tells a folder that is not a repository apart from a git failure", async () => {
    setup(snapshot, overview({ is_repo: false, branch: null, head: null }));
    expect((await screen.findByRole("button", { name: /Not a git repo/ })).className).toContain("git-chip--none");
    cleanup();
    setup(snapshot, overview(), { gitOverview: vi.fn().mockRejectedValue("boom") });
    expect((await screen.findByRole("button", { name: /Git unavailable/ })).className).toContain("git-chip--alert");
  });
});
