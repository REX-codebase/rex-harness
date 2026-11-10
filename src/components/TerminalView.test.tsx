// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// xterm needs a real canvas; a stub with the same surface is enough to test the wiring.
const term = vi.hoisted(() => ({ instances: [] as unknown[] }));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 100;
    rows = 30;
    written: string[] = [];
    disposed = false;
    focused = 0;
    onDataHandler: ((d: string) => void) | null = null;
    constructor() {
      term.instances.push(this);
    }
    loadAddon() {}
    open() {}
    write(d: string) {
      this.written.push(d);
    }
    onData(cb: (d: string) => void) {
      this.onDataHandler = cb;
    }
    focus() {
      this.focused += 1;
    }
    dispose() {
      this.disposed = true;
    }
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit = vi.fn(); } }));
vi.mock("@xterm/xterm/css/xterm.css", () => ({}));

const bridge = vi.hoisted(() => ({
  outputs: new Map<string, (d: string) => void>(),
  exits: new Map<string, () => void>(),
  spawn: vi.fn(),
  write: vi.fn(),
  resize: vi.fn(),
  kill: vi.fn(),
}));
vi.mock("../data/terminal", () => ({
  terminalSpawn: (...a: unknown[]) => bridge.spawn(...a),
  terminalWrite: (...a: unknown[]) => bridge.write(...a),
  terminalResize: (...a: unknown[]) => bridge.resize(...a),
  terminalKill: (...a: unknown[]) => bridge.kill(...a),
  onTerminalOutput: async (id: string, cb: (d: string) => void) => {
    bridge.outputs.set(id, cb);
    return () => bridge.outputs.delete(id);
  },
  onTerminalExit: async (id: string, cb: () => void) => {
    bridge.exits.set(id, cb);
    return () => bridge.exits.delete(id);
  },
}));

import { WorkspaceProvider, type WorkspaceApi } from "../data/workspaceContext";
import { TerminalView } from "./TerminalView";

// jsdom has no ResizeObserver or layout. Record observers so a test can trigger one.
const observers: Array<() => void> = [];
class FakeResizeObserver {
  constructor(cb: () => void) {
    observers.push(cb);
  }
  observe() {}
  disconnect() {}
}
(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = FakeResizeObserver;

const ROOT = "/work/app";
type FakeTerm = { written: string[]; disposed: boolean; focused: number; onDataHandler: ((d: string) => void) | null };
const latest = () => term.instances[term.instances.length - 1] as FakeTerm;

function renderTerminal(opts: { folder?: string | null; active?: boolean } = {}) {
  const folder = opts.folder === undefined ? ROOT : opts.folder;
  const api: WorkspaceApi = {
    state: async () => ({ active: folder ? { path: folder, name: folder.split("/").pop()!, exists: true } : null, recent: [], runs: [] }),
    pick: vi.fn().mockResolvedValue(null),
    activate: vi.fn(),
    forget: vi.fn(),
    gitOverview: async () => ({ git_available: false, is_repo: false } as never),
  } as never;
  const tree = (active: boolean) => (
    <WorkspaceProvider api={api} desktop pollMs={60_000}>
      <TerminalView active={active} />
    </WorkspaceProvider>
  );
  const view = render(tree(opts.active ?? true));
  return { ...view, setActive: (a: boolean) => view.rerender(tree(a)) };
}

let nextId = 0;
beforeEach(() => {
  term.instances.length = 0;
  observers.length = 0;
  bridge.outputs.clear();
  bridge.exits.clear();
  nextId = 0;
  bridge.spawn.mockReset().mockImplementation(async () => `term-${++nextId}`);
  bridge.write.mockReset().mockResolvedValue(undefined);
  bridge.resize.mockReset().mockResolvedValue(undefined);
  bridge.kill.mockReset().mockResolvedValue(undefined);
});
afterEach(cleanup);

describe("starting a shell", () => {
  it("asks for a folder first, and does not spawn anything", async () => {
    renderTerminal({ folder: null });
    expect(await screen.findByText("Open a folder to start a terminal")).toBeTruthy();
    expect(bridge.spawn).not.toHaveBeenCalled();
  });

  it("opens a shell in the open folder at the size of the view", async () => {
    renderTerminal();
    await waitFor(() => expect(bridge.spawn).toHaveBeenCalledWith(ROOT, 100, 30));
    expect(await screen.findByText("running")).toBeTruthy();
    expect(screen.getByTitle(ROOT).textContent).toBe("app");
  });

  it("shows why a shell could not start, and Restart tries again", async () => {
    bridge.spawn.mockRejectedValueOnce("cannot spawn shell: no such file");
    renderTerminal();
    expect((await screen.findByRole("alert")).textContent).toContain("no such file");
    expect(screen.getByText("could not start")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Restart" }));
    await waitFor(() => expect(screen.getByText("running")).toBeTruthy());
    expect(bridge.spawn).toHaveBeenCalledTimes(2);
  });
});

describe("output and input", () => {
  it("draws what the shell prints and sends what the person types", async () => {
    renderTerminal();
    await screen.findByText("running");
    act(() => bridge.outputs.get("term-1")!("hello ünï\r\n"));
    expect(latest().written).toEqual(["hello ünï\r\n"]);
    latest().onDataHandler!("ls\r");
    expect(bridge.write).toHaveBeenCalledWith("term-1", "ls\r");
  });
});

describe("resizing", () => {
  it("tells the shell its new size, but not while the tab is hidden (a hidden view has no size)", async () => {
    const size = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
    const height = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(500);
    try {
      const { setActive } = renderTerminal();
      await screen.findByText("running");
      bridge.resize.mockClear();
      observers.forEach((cb) => cb());
      expect(bridge.resize).toHaveBeenCalledWith("term-1", 100, 30);

      size.mockReturnValue(0);
      height.mockReturnValue(0);
      setActive(false);
      bridge.resize.mockClear();
      observers.forEach((cb) => cb());
      expect(bridge.resize).not.toHaveBeenCalled();
    } finally {
      size.mockRestore();
      height.mockRestore();
    }
  });
});

describe("when the shell ends", () => {
  it("says so and offers Restart, which starts a fresh shell in the same folder", async () => {
    renderTerminal();
    await screen.findByText("running");
    act(() => bridge.exits.get("term-1")!());
    expect(await screen.findByText("shell exited")).toBeTruthy();
    // typing into a dead shell goes nowhere
    latest().onDataHandler!("x");
    expect(bridge.write).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Restart" }));
    await waitFor(() => expect(bridge.spawn).toHaveBeenCalledTimes(2));
    expect(bridge.spawn).toHaveBeenLastCalledWith(ROOT, 100, 30);
    expect(await screen.findByText("running")).toBeTruthy();
    expect(term.instances.length).toBe(2);
    expect((term.instances[0] as FakeTerm).disposed).toBe(true);
  });
});

describe("switching tabs", () => {
  it("keeps the same shell alive and only hides it, then refits and focuses on return", async () => {
    const { setActive, container } = renderTerminal();
    await screen.findByText("running");
    const shell = container.querySelector(".terminal-view")!;
    expect(shell.hasAttribute("hidden")).toBe(false);

    setActive(false);
    expect(shell.hasAttribute("hidden")).toBe(true);
    expect(bridge.kill).not.toHaveBeenCalled();
    expect(bridge.spawn).toHaveBeenCalledTimes(1);
    expect(latest().disposed).toBe(false);

    const focusedBefore = latest().focused;
    setActive(true);
    expect(shell.hasAttribute("hidden")).toBe(false);
    await waitFor(() => expect(latest().focused).toBeGreaterThan(focusedBefore));
    expect(bridge.spawn).toHaveBeenCalledTimes(1);
  });
});

describe("closing", () => {
  it("kills the shell when the view goes away, not before", async () => {
    const { unmount } = renderTerminal();
    await screen.findByText("running");
    expect(bridge.kill).not.toHaveBeenCalled();
    unmount();
    expect(bridge.kill).toHaveBeenCalledWith("term-1");
    expect(latest().disposed).toBe(true);
  });

  it("kills a shell that finished starting only after the view had already gone", async () => {
    let finish: (id: string) => void = () => undefined;
    bridge.spawn.mockReturnValue(new Promise<string>((r) => (finish = r)));
    const { unmount } = renderTerminal();
    await waitFor(() => expect(bridge.spawn).toHaveBeenCalled());
    unmount();
    await act(async () => finish("term-late"));
    await waitFor(() => expect(bridge.kill).toHaveBeenCalledWith("term-late"));
  });
});
