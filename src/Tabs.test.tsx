// @vitest-environment jsdom
//
// The top-bar tabs through the real App, with the desktop bridge replaced by a
// canned backend. These check what a person sees: the folder and git status on
// every tab, and that the terminal and the editor survive switching tabs.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({
  calls: [] as { cmd: string; args: unknown }[],
  handlers: {} as Record<string, (args: any) => unknown>,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string, args: unknown) => {
    backend.calls.push({ cmd, args });
    const handler = backend.handlers[cmd];
    if (!handler) throw new Error(`not mocked: ${cmd}`);
    return handler(args);
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
    write() {}
    onData() {}
    focus() {}
    dispose() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));
vi.mock("@xterm/xterm/css/xterm.css", () => ({}));
vi.mock("@monaco-editor/react", () => ({
  default: ({ value, onChange, path }: { value: string; onChange: (v: string) => void; path: string }) => (
    <textarea aria-label="Editor text" data-path={path} value={value} onChange={(e) => onChange(e.target.value)} />
  ),
  DiffEditor: () => null,
}));
vi.mock("./lib/monaco", () => ({ useMonacoReady: () => true, ensureMonaco: async () => undefined }));

import App from "./App";

const ROOT = "/work/app";
const count = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd).length;
// a string name is matched in full, so "Git" does not match "Git status: …"
const tab = (name: string) => screen.getByRole("button", { name });

beforeEach(() => {
  backend.calls.length = 0;
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
  window.localStorage.clear();
  window.localStorage.setItem("rex-operator-mode", "human");
  Element.prototype.scrollIntoView = vi.fn();
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class { observe() {} disconnect() {} };
  let terminals = 0;
  backend.handlers = {
    workspace_state: () => ({ active: { path: ROOT, name: "app", exists: true }, recent: [{ path: ROOT, name: "app", exists: true }], runs: [] }),
    git_overview: () => ({
      git_available: true, is_repo: true, root: ROOT, branch: "main", detached: false, head: "abc1234", upstream: "origin/main",
      ahead: 1, behind: 0, staged: 0, unstaged: 2, untracked: 0, conflicted: 0, truncated: false, message: null,
      files: [
        { path: "a.txt", orig_path: null, index: ".", worktree: "M", untracked: false, conflicted: false },
        { path: "b.txt", orig_path: null, index: ".", worktree: "M", untracked: false, conflicted: false },
      ],
    }),
    git_log: () => [],
    terminal_spawn: () => `term-${++terminals}`,
    terminal_kill: () => undefined,
    terminal_resize: () => undefined,
    terminal_write: () => undefined,
    workspace_tree: () => ({ files: ["README.md", "src/main.rs"], truncated: false }),
    workspace_read_file: ({ path }: { path: string }) => ({ content: `contents of ${path}`, size: 10 }),
    checkpoint_list: () => [],
    app_info: () => ({ version: "0.1.2", data_dir: "/home/a/.config/rex-harness", runs_root: "/home/a/.config/rex-harness/agent-runs", platform: "linux", git_available: true }),
  };
});
afterEach(() => {
  cleanup();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("the folder and git status on every tab", () => {
  it("shows the open folder and the branch with its change count, and the git chip opens the Git tab", async () => {
    render(<App />);
    expect(await screen.findByRole("button", { name: "Folder: app. Change folder" })).toBeTruthy();
    const chip = await screen.findByRole("button", { name: /Git status: main ↑1 · 2 changes/ });
    expect(chip.className).toContain("git-chip--dirty");

    fireEvent.click(chip);
    const changes = await screen.findByRole("region", { name: "Changes" });
    expect(within(changes).getByText("a.txt")).toBeTruthy();
    // the chip is still there on the Git tab, and on the others
    fireEvent.click(tab("Settings"));
    expect(screen.getByRole("button", { name: /Git status: main/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Folder: app. Change folder" })).toBeTruthy();
  });

  it("shows no folder or git chip until a folder is open", async () => {
    backend.handlers.workspace_state = () => ({ active: null, recent: [], runs: [] });
    render(<App />);
    expect(await screen.findByRole("button", { name: "Open a folder" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Git status/ })).toBeNull();
    fireEvent.click(tab("Git"));
    expect(await screen.findByText("Open a folder to see its git status")).toBeTruthy();
    fireEvent.click(tab("Terminal"));
    expect(await screen.findByText("Open a folder to start a terminal")).toBeTruthy();
    fireEvent.click(tab("Editor"));
    expect(await screen.findByText("Open a folder to edit its files")).toBeTruthy();
  });
});

describe("tabs keep their state", () => {
  it("starts one shell and keeps it across trips to other tabs", async () => {
    render(<App />);
    fireEvent.click(tab("Terminal"));
    await waitFor(() => expect(count("terminal_spawn")).toBe(1));
    expect(backend.calls.find((c) => c.cmd === "terminal_spawn")?.args).toMatchObject({ workspace: ROOT });

    fireEvent.click(tab("Editor"));
    fireEvent.click(tab("Git"));
    fireEvent.click(tab("Task"));
    fireEvent.click(tab("Terminal"));
    await new Promise((r) => setTimeout(r, 30));
    expect(count("terminal_spawn")).toBe(1);
    expect(count("terminal_kill")).toBe(0);
    expect(screen.getByLabelText("Workspace terminal").hasAttribute("hidden")).toBe(false);
    fireEvent.click(tab("Task"));
    expect(screen.getByLabelText("Workspace terminal").hasAttribute("hidden")).toBe(true);
  });

  it("keeps the open file and its unsaved edit when you leave the Editor and come back", async () => {
    render(<App />);
    fireEvent.click(tab("Editor"));
    fireEvent.click(await screen.findByRole("button", { name: "README.md" }));
    const box = (await screen.findByLabelText("Editor text")) as HTMLTextAreaElement;
    await waitFor(() => expect(box.value).toBe("contents of README.md"));
    fireEvent.change(box, { target: { value: "my unsaved words" } });

    fireEvent.click(tab("Git"));
    fireEvent.click(tab("Task"));
    fireEvent.click(tab("Editor"));
    expect((screen.getByLabelText("Editor text") as HTMLTextAreaElement).value).toBe("my unsaved words");
    expect(screen.getByRole("button", { name: "Save ●" })).toBeTruthy();
    // and the file was not read a second time
    expect(count("workspace_read_file")).toBe(1);
  });

  it("does not start a shell or read files for tabs that were never opened", async () => {
    render(<App />);
    await screen.findByRole("button", { name: /Git status/ });
    fireEvent.click(tab("Settings"));
    expect(count("terminal_spawn")).toBe(0);
    expect(count("workspace_tree")).toBe(0);
  });
});

describe("settings", () => {
  it("shows the version and where the data lives", async () => {
    render(<App />);
    fireEvent.click(tab("Settings"));
    const about = await screen.findByRole("region", { name: "About" });
    expect(await within(about).findByText("/home/a/.config/rex-harness")).toBeTruthy();
    expect(within(about).getByText("0.1.2")).toBeTruthy();
    expect(within(about).getByText("Found on this computer")).toBeTruthy();
  });
});

describe("in a browser tab", () => {
  it("explains that these tabs need the desktop app instead of failing", async () => {
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    render(<App />);
    for (const [name, text] of [["Terminal", /terminal runs in the desktop app/], ["Editor", /editor runs in the desktop app/], ["Git", /Git and checkpoints run in the desktop app/]] as const) {
      fireEvent.click(tab(name));
      expect(await screen.findByText(text)).toBeTruthy();
    }
    expect(screen.queryByRole("button", { name: /Git status/ })).toBeNull();
    expect(backend.calls.length).toBe(0);
  });
});
