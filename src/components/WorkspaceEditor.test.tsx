// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Monaco cannot run in jsdom; a textarea keeps the same contract (value, onChange).
vi.mock("@monaco-editor/react", () => ({
  default: ({ value, onChange, path, language }: { value: string; onChange: (v: string) => void; path: string; language: string }) => (
    <textarea aria-label="Editor text" data-path={path} data-language={language} value={value} onChange={(e) => onChange(e.target.value)} />
  ),
}));
vi.mock("../lib/monaco", () => ({ useMonacoReady: () => true, ensureMonaco: async () => undefined }));
vi.mock("../data/workspace", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data/workspace")>()),
  workspaceTree: vi.fn(),
  workspaceReadFile: vi.fn(),
  workspaceWriteFile: vi.fn(),
  workspaceCreateFile: vi.fn(),
}));

import * as ws from "../data/workspace";
import { WorkspaceProvider, useWorkspace, type WorkspaceApi } from "../data/workspaceContext";
import { WorkspaceEditor } from "./WorkspaceEditor";

const m = (fn: unknown) => fn as ReturnType<typeof vi.fn>;
const ROOT = "/work/app";
const files: Record<string, string> = { "README.md": "# hello", "src/main.rs": "fn main() {}", "src/lib.rs": "pub fn x() {}" };

let activate: ReturnType<typeof vi.fn>;
let probe: ReturnType<typeof useWorkspace>;
function Probe() {
  probe = useWorkspace();
  return null;
}

function renderEditor(opts: { active?: boolean; folder?: boolean } = {}) {
  const folder = opts.folder ?? true;
  activate = vi.fn().mockResolvedValue({ active: { path: "/work/other", name: "other", exists: true }, recent: [], runs: [] });
  const api: WorkspaceApi = {
    state: async () => ({ active: folder ? { path: ROOT, name: "app", exists: true } : null, recent: [], runs: [] }),
    pick: vi.fn().mockResolvedValue(null),
    activate,
    forget: vi.fn(),
    gitOverview: async () => ({ git_available: false, is_repo: false } as never),
  } as never;
  const tree = (active: boolean) => (
    <WorkspaceProvider api={api} desktop pollMs={60_000}>
      <Probe />
      <WorkspaceEditor active={active} />
    </WorkspaceProvider>
  );
  const view = render(tree(opts.active ?? true));
  return { ...view, setActive: (active: boolean) => view.rerender(tree(active)) };
}

const textarea = () => screen.getByLabelText("Editor text") as HTMLTextAreaElement;
const open = async (name: string) => {
  fireEvent.click(await screen.findByRole("button", { name }));
  await waitFor(() => expect(textarea().dataset.path).toBe(name));
};

beforeEach(() => {
  m(ws.workspaceTree).mockReset().mockResolvedValue({ files: Object.keys(files), truncated: false });
  m(ws.workspaceReadFile).mockReset().mockImplementation(async (_w: string, p: string) => {
    if (!(p in files)) throw "no such file";
    return { content: files[p], size: files[p].length };
  });
  m(ws.workspaceWriteFile).mockReset().mockResolvedValue(undefined);
  m(ws.workspaceCreateFile).mockReset().mockResolvedValue(undefined);
  vi.spyOn(window, "confirm").mockReturnValue(true);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("before a folder is open", () => {
  it("asks for one", async () => {
    renderEditor({ folder: false });
    expect(await screen.findByText("Open a folder to edit its files")).toBeTruthy();
  });
  it("draws nothing for a tab that is not on screen", async () => {
    const { container } = renderEditor({ folder: false, active: false });
    await new Promise((r) => setTimeout(r, 20));
    expect(container.querySelector(".no-workspace")).toBeNull();
  });
});

describe("the file list", () => {
  it("shows every file in the folder, nested ones included, and filters them", async () => {
    renderEditor();
    expect(await screen.findByRole("button", { name: "src/main.rs" })).toBeTruthy();
    expect(screen.getAllByRole("button").filter((b) => b.title.includes("/")).length).toBe(2);
    fireEvent.change(screen.getByLabelText("Filter files"), { target: { value: "lib" } });
    expect(screen.queryByRole("button", { name: "README.md" })).toBeNull();
    expect(screen.getByRole("button", { name: "src/lib.rs" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Filter files"), { target: { value: "zzz" } });
    expect(screen.getByText(/No file matches/)).toBeTruthy();
  });

  it("says so for an empty folder, and when the list was cut short", async () => {
    m(ws.workspaceTree).mockResolvedValue({ files: [], truncated: false });
    renderEditor();
    expect(await screen.findByText(/no files yet/)).toBeTruthy();
    cleanup();
    m(ws.workspaceTree).mockResolvedValue({ files: ["a.txt"], truncated: true });
    renderEditor();
    expect(await screen.findByText(/Showing the first 1 files/)).toBeTruthy();
  });

  it("shows why the list could not be read", async () => {
    m(ws.workspaceTree).mockRejectedValue("the folder no longer exists");
    renderEditor();
    expect((await screen.findByRole("alert")).textContent).toBe("the folder no longer exists");
  });
});

describe("editing and saving", () => {
  it("opens a file with the right language and nothing to save yet", async () => {
    renderEditor();
    await open("src/main.rs");
    expect(textarea().value).toBe("fn main() {}");
    expect(textarea().dataset.language).toBe("rust");
    expect((screen.getByRole("button", { name: "Saved" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("marks an edit as unsaved and writes it with Save", async () => {
    renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# changed" } });
    const save = screen.getByRole("button", { name: "Save ●" }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => expect(ws.workspaceWriteFile).toHaveBeenCalledWith(ROOT, "README.md", "# changed"));
    expect(await screen.findByRole("button", { name: "Saved" })).toBeTruthy();
    expect(screen.getByText("Saved README.md")).toBeTruthy();
  });

  it("saves with Ctrl+S, and the browser's own save is suppressed", async () => {
    renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# via keyboard" } });
    const proceeded = fireEvent.keyDown(textarea(), { key: "s", ctrlKey: true });
    expect(proceeded).toBe(false);
    await waitFor(() => expect(ws.workspaceWriteFile).toHaveBeenCalledWith(ROOT, "README.md", "# via keyboard"));
  });

  it("keeps the edit and shows the reason when a save fails", async () => {
    m(ws.workspaceWriteFile).mockRejectedValue("cannot save the file: permission denied");
    renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# will not save" } });
    fireEvent.click(screen.getByRole("button", { name: "Save ●" }));
    expect((await screen.findByRole("alert")).textContent).toContain("permission denied");
    expect(textarea().value).toBe("# will not save");
    expect(screen.getByRole("button", { name: "Save ●" })).toBeTruthy();
  });
});

describe("unsaved changes", () => {
  it("asks before switching files, and stays on the edit if the person says no", async () => {
    renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# edited" } });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValue(false);
    fireEvent.click(screen.getByRole("button", { name: "src/main.rs" }));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("README.md"));
    expect(textarea().dataset.path).toBe("README.md");
    expect(textarea().value).toBe("# edited");
    expect(ws.workspaceReadFile).toHaveBeenCalledTimes(1);

    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: "src/main.rs" }));
    await waitFor(() => expect(textarea().dataset.path).toBe("src/main.rs"));
  });

  it("does not ask when there is nothing to lose", async () => {
    renderEditor();
    await open("README.md");
    await open("src/main.rs");
    expect(window.confirm).not.toHaveBeenCalled();
  });

  it("asks before the folder is switched, through the shared guard", async () => {
    renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# edited" } });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValue(false);
    await act(async () => {
      await probe.activate("/work/other");
    });
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("unsaved changes"));
    expect(activate).not.toHaveBeenCalled();
    // once saved, switching is free
    (window.confirm as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Save ●" }));
    await screen.findByRole("button", { name: "Saved" });
    await act(async () => {
      await probe.activate("/work/other");
    });
    expect(window.confirm).not.toHaveBeenCalled();
    expect(activate).toHaveBeenCalledWith("/work/other");
  });

  it("warns on closing the window only while there are unsaved edits", async () => {
    renderEditor();
    await open("README.md");
    const clean = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(clean);
    expect(clean.defaultPrevented).toBe(false);
    fireEvent.change(textarea(), { target: { value: "# edited" } });
    const dirty = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(dirty);
    expect(dirty.defaultPrevented).toBe(true);
  });
});

describe("files the editor will not open", () => {
  it("explains why, and leaves the file that was open alone", async () => {
    files["image.png"] = "";
    m(ws.workspaceTree).mockResolvedValue({ files: Object.keys(files), truncated: false });
    m(ws.workspaceReadFile).mockImplementation(async (_w: string, p: string) => {
      if (p === "image.png") throw "This looks like a binary file; the editor opens text files only.";
      return { content: files[p], size: 1 };
    });
    renderEditor();
    await open("README.md");
    fireEvent.click(screen.getByRole("button", { name: "image.png" }));
    expect((await screen.findByRole("alert")).textContent).toContain("binary file");
    expect(textarea().dataset.path).toBe("README.md");
    delete files["image.png"];
  });
});

describe("new files", () => {
  it("creates one, refreshes the list and opens it", async () => {
    renderEditor();
    await screen.findByRole("button", { name: "README.md" });
    fireEvent.click(screen.getByRole("button", { name: "+" }));
    const input = screen.getByLabelText("New file path");
    expect((screen.getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "docs\\notes.md" } });
    files["docs/notes.md"] = "";
    m(ws.workspaceTree).mockResolvedValue({ files: Object.keys(files), truncated: false });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(ws.workspaceCreateFile).toHaveBeenCalledWith(ROOT, "docs\\notes.md"));
    await waitFor(() => expect(textarea().dataset.path).toBe("docs/notes.md"));
    expect(screen.getByText("Created docs/notes.md")).toBeTruthy();
    delete files["docs/notes.md"];
  });

  it("shows the backend's refusal, for example a name that already exists", async () => {
    m(ws.workspaceCreateFile).mockRejectedValue("a file with that name already exists");
    renderEditor();
    await screen.findByRole("button", { name: "README.md" });
    fireEvent.click(screen.getByRole("button", { name: "+" }));
    fireEvent.change(screen.getByLabelText("New file path"), { target: { value: "README.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect((await screen.findByRole("alert")).textContent).toContain("already exists");
  });
});

describe("staying mounted", () => {
  it("keeps its state, hidden, when its tab is not the one on screen", async () => {
    const { setActive } = renderEditor();
    await open("README.md");
    fireEvent.change(textarea(), { target: { value: "# edited" } });
    const root = screen.getByLabelText("Workspace editor");
    expect(root.hasAttribute("hidden")).toBe(false);
    setActive(false);
    expect(screen.getByLabelText("Workspace editor").hasAttribute("hidden")).toBe(true);
    expect(textarea().value).toBe("# edited");
  });
});
