// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceProvider, useWorkspace, type WorkspaceApi } from "./workspaceContext";
import type { WorkspaceSnapshot } from "./workspace";
import type { GitOverview } from "./git";

const info = (name: string) => ({ path: `/p/${name}`, name, exists: true });
const snap = (active: string | null, ...recent: string[]): WorkspaceSnapshot => ({
  active: active ? info(active) : null,
  recent: recent.map(info),
  runs: [],
});
const overview = (over: Partial<GitOverview> = {}): GitOverview => ({
  git_available: true, is_repo: true, root: "/p", branch: "main", detached: false, head: "abc1234", upstream: null,
  ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, files: [], truncated: false, message: null,
  ...over,
});

type FakeApi = { [K in keyof WorkspaceApi]: ReturnType<typeof vi.fn> };
function fakeApi(over: Partial<WorkspaceApi> = {}): FakeApi {
  return {
    state: vi.fn().mockResolvedValue(snap("alpha", "alpha", "beta")),
    pick: vi.fn().mockResolvedValue(snap("gamma", "gamma", "alpha", "beta")),
    activate: vi.fn().mockImplementation(async (p: string) => snap(p.split("/").pop()!, "alpha", "beta")),
    forget: vi.fn().mockResolvedValue(snap(null, "beta")),
    gitOverview: vi.fn().mockResolvedValue(overview()),
    ...over,
  } as never;
}

let ws: ReturnType<typeof useWorkspace>;
function Probe() {
  ws = useWorkspace();
  return (
    <p>
      {ws.active?.name ?? "none"}|{ws.git ? `${ws.git.branch}:${ws.git.unstaged}` : "nogit"}|{ws.gitError ?? ""}|{ws.error ?? ""}
    </p>
  );
}
const show = () => screen.getByText(/\|/).textContent;
// the mocks are vi.fn()s; the provider only needs them to behave like WorkspaceApi
const mount = (api: FakeApi, props: Record<string, unknown> = {}) =>
  render(
    <WorkspaceProvider api={api as unknown as WorkspaceApi} desktop pollMs={1000} {...props}>
      <Probe />
    </WorkspaceProvider>,
  );

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("the open folder and its git state", () => {
  it("loads the folder list, then reads git for the active folder", async () => {
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    expect(api.gitOverview).toHaveBeenCalledWith("/p/alpha");
  });

  it("is inert outside the desktop app: nothing is requested", async () => {
    const api = fakeApi();
    mount(api, { desktop: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(show()).toBe("none|nogit||");
    expect(api.state).not.toHaveBeenCalled();
    expect(api.gitOverview).not.toHaveBeenCalled();
  });

  it("reads git again on a timer while the window is in view, and sees new changes", async () => {
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    api.gitOverview.mockResolvedValue(overview({ unstaged: 3 }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
    });
    await waitFor(() => expect(show()).toBe("alpha|main:3||"));
  });

  it("does not poll while the window is hidden", async () => {
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(api.gitOverview).toHaveBeenCalledTimes(1));
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" });
    try {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000);
      });
      expect(api.gitOverview).toHaveBeenCalledTimes(1);
    } finally {
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
    }
  });

  it("reads git as soon as the window regains focus", async () => {
    const api = fakeApi();
    mount(api, { pollMs: 60_000 });
    await waitFor(() => expect(api.gitOverview).toHaveBeenCalledTimes(1));
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(api.gitOverview).toHaveBeenCalledTimes(2));
  });

  it("never runs two git reads at once, however often it is asked", async () => {
    let release: (o: GitOverview) => void = () => undefined;
    const api = fakeApi({ gitOverview: vi.fn().mockReturnValue(new Promise<GitOverview>((r) => (release = r))) });
    mount(api, { pollMs: 50 });
    // the poll timer exists only once the folder is known, so wait for the first read
    await waitFor(() => expect(api.gitOverview).toHaveBeenCalledTimes(1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(api.gitOverview).toHaveBeenCalledTimes(1);
    await act(async () => release(overview({ unstaged: 1 })));
    await waitFor(() => expect(show()).toBe("alpha|main:1||"));
  });

  it("ignores a slow answer for a folder that is no longer open", async () => {
    let releaseAlpha: (o: GitOverview) => void = () => undefined;
    const gitOverview = vi.fn().mockImplementation((p: string) =>
      p.endsWith("alpha") ? new Promise<GitOverview>((r) => (releaseAlpha = r)) : Promise.resolve(overview({ branch: "beta-branch" })),
    );
    const api = fakeApi({ gitOverview });
    mount(api, { pollMs: 60_000 });
    await waitFor(() => expect(gitOverview).toHaveBeenCalledWith("/p/alpha"));
    await act(async () => {
      await ws.activate("/p/beta");
    });
    await waitFor(() => expect(show()).toBe("beta|beta-branch:0||"));
    // alpha's answer arrives late and must not overwrite beta's
    await act(async () => releaseAlpha(overview({ branch: "alpha-branch" })));
    expect(show()).toBe("beta|beta-branch:0||");
  });

  it("keeps a git failure apart from 'not a repository'", async () => {
    const api = fakeApi({ gitOverview: vi.fn().mockRejectedValue("backend exploded") });
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|nogit|backend exploded|"));
  });

  it("shows why the folder list could not be read", async () => {
    mount(fakeApi({ state: vi.fn().mockRejectedValue(new Error("no store")) }));
    await waitFor(() => expect(show()).toBe("none|nogit||no store"));
  });

  it("leaves everything as it was when the person cancels the folder dialog", async () => {
    const api = fakeApi({ pick: vi.fn().mockResolvedValue(null) });
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    await act(async () => {
      await ws.pick();
    });
    expect(show()).toBe("alpha|main:0||");
  });

  it("switches to a picked folder and reads its git", async () => {
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    await act(async () => {
      await ws.pick();
    });
    await waitFor(() => expect(api.gitOverview).toHaveBeenCalledWith("/p/gamma"));
    expect(show().startsWith("gamma|")).toBe(true);
  });
});

describe("leaving a folder with unsaved edits", () => {
  it("asks first, and stays put when the person says no", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    act(() => ws.reportUnsaved("editor", true));
    await act(async () => {
      await ws.activate("/p/beta");
      await ws.pick();
    });
    expect(confirm).toHaveBeenCalledTimes(2);
    expect(api.activate).not.toHaveBeenCalled();
    expect(api.pick).not.toHaveBeenCalled();
    expect(show().startsWith("alpha|")).toBe(true);
  });

  it("goes ahead when they agree, and does not ask once the edits are saved", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    act(() => ws.reportUnsaved("editor", true));
    await act(async () => {
      await ws.activate("/p/beta");
    });
    expect(api.activate).toHaveBeenCalledWith("/p/beta");
    act(() => ws.reportUnsaved("editor", false));
    confirm.mockClear();
    await act(async () => {
      await ws.activate("/p/alpha");
    });
    expect(confirm).not.toHaveBeenCalled();
  });

  it("does not ask when 'switching' to the folder that is already open", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const api = fakeApi();
    mount(api);
    await waitFor(() => expect(show()).toBe("alpha|main:0||"));
    act(() => ws.reportUnsaved("editor", true));
    await act(async () => {
      await ws.activate("/p/alpha");
    });
    expect(confirm).not.toHaveBeenCalled();
    expect(api.activate).not.toHaveBeenCalled();
  });
});
