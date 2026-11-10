import { describe, expect, it } from "vitest";
import { branchName, changeLetter, changeWord, gitChip, groupChanges, syncLabel, type GitChange, type GitOverview } from "./git";
import { diffLineKind } from "../components/GitPanel";

const change = (over: Partial<GitChange>): GitChange => ({
  path: "a.txt",
  orig_path: null,
  index: ".",
  worktree: ".",
  untracked: false,
  conflicted: false,
  ...over,
});

const overview = (over: Partial<GitOverview> = {}): GitOverview => ({
  git_available: true,
  is_repo: true,
  root: "/p",
  branch: "main",
  detached: false,
  head: "abc1234",
  upstream: null,
  ahead: 0,
  behind: 0,
  staged: 0,
  unstaged: 0,
  untracked: 0,
  conflicted: 0,
  files: [],
  truncated: false,
  message: null,
  ...over,
});

describe("groupChanges", () => {
  it("puts a file edited after staging in both Staged and Changes", () => {
    const g = groupChanges([change({ path: "both.txt", index: "M", worktree: "M" })]);
    expect(g.staged.map((c) => c.path)).toEqual(["both.txt"]);
    expect(g.unstaged.map((c) => c.path)).toEqual(["both.txt"]);
  });

  it("keeps conflicts and untracked files out of the staged and unstaged lists", () => {
    const g = groupChanges([
      change({ path: "clash.txt", index: "U", worktree: "U", conflicted: true }),
      change({ path: "new.txt", untracked: true }),
      change({ path: "staged.txt", index: "A" }),
      change({ path: "edited.txt", worktree: "M" }),
    ]);
    expect(g.conflicted.map((c) => c.path)).toEqual(["clash.txt"]);
    expect(g.untracked.map((c) => c.path)).toEqual(["new.txt"]);
    expect(g.staged.map((c) => c.path)).toEqual(["staged.txt"]);
    expect(g.unstaged.map((c) => c.path)).toEqual(["edited.txt"]);
  });
});

describe("the status chip", () => {
  it("says it is reading before the first answer", () => {
    expect(gitChip(null)).toMatchObject({ tone: "idle", label: "Git…" });
  });

  it("reports a clean branch", () => {
    const chip = gitChip(overview());
    expect(chip).toMatchObject({ tone: "clean", label: "main · clean" });
    expect(chip.title).toBe("main: nothing to commit");
  });

  it("counts every kind of change and explains them in the tooltip", () => {
    const chip = gitChip(overview({ staged: 2, unstaged: 1, untracked: 3 }));
    expect(chip.tone).toBe("dirty");
    expect(chip.label).toBe("main · 6 changes");
    expect(chip.title).toBe("main: 2 staged, 1 modified, 3 untracked");
    expect(gitChip(overview({ untracked: 1 })).label).toBe("main · 1 change");
  });

  it("shows how far ahead of or behind the upstream the branch is", () => {
    const chip = gitChip(overview({ ahead: 2, behind: 1, upstream: "origin/main", unstaged: 1 }));
    expect(chip.label).toBe("main ↑2 ↓1 · 1 change");
    expect(chip.title).toBe("main: 1 modified (2 ahead of origin/main, 1 behind origin/main)");
    expect(gitChip(overview({ behind: 3 })).label).toBe("main ↓3 · clean");
  });

  it("raises an alert for conflicts, even when other things changed", () => {
    const chip = gitChip(overview({ conflicted: 2, unstaged: 5 }));
    expect(chip).toMatchObject({ tone: "alert", label: "main · 2 conflicts" });
    expect(gitChip(overview({ conflicted: 1 })).label).toBe("main · 1 conflict");
  });

  it("names a detached HEAD and a repository with no commits instead of a blank branch", () => {
    expect(gitChip(overview({ branch: null, detached: true, head: "abc1234" })).label).toBe("detached at abc1234 · clean");
    expect(gitChip(overview({ branch: null, detached: false, head: null })).label).toBe("no commits yet · clean");
    expect(branchName(overview({ branch: null, detached: true, head: null }))).toBe("detached HEAD");
  });

  it("treats a folder outside any repository as normal, not as an error", () => {
    const chip = gitChip(overview({ is_repo: false, branch: null, head: null }));
    expect(chip.tone).toBe("none");
    expect(chip.label).toBe("Not a git repo");
  });

  it("says when git is missing, and surfaces a git failure as an alert", () => {
    expect(gitChip(overview({ git_available: false, is_repo: false, message: "Git is not installed." }))).toMatchObject({ tone: "none", label: "Git not installed" });
    expect(gitChip(overview({ is_repo: false, message: "fatal: detected dubious ownership" }))).toMatchObject({ tone: "alert", label: "Git unavailable" });
    expect(gitChip(null, "backend gone")).toMatchObject({ tone: "alert", title: "backend gone" });
  });
});

describe("small helpers", () => {
  it("formats the sync arrows", () => {
    expect(syncLabel({ ahead: 0, behind: 0 })).toBe("");
    expect(syncLabel({ ahead: 4, behind: 0 })).toBe("↑4");
    expect(syncLabel({ ahead: 1, behind: 2 })).toBe("↑1 ↓2");
  });
  it("words the change letters", () => {
    expect(changeLetter(".")).toBe("");
    expect(changeLetter("M")).toBe("M");
    expect(changeWord("A")).toBe("Added");
    expect(changeWord("R")).toBe("Renamed");
    expect(changeWord("Z")).toBe("Changed");
  });
  it("classifies diff lines for colouring", () => {
    expect(diffLineKind("@@ -1,2 +1,3 @@")).toBe("hunk");
    expect(diffLineKind("+++ b/a.txt")).toBe("meta");
    expect(diffLineKind("--- a/a.txt")).toBe("meta");
    expect(diffLineKind("diff --git a/a b/a")).toBe("meta");
    expect(diffLineKind("+added")).toBe("add");
    expect(diffLineKind("-removed")).toBe("del");
    expect(diffLineKind(" context")).toBe("ctx");
  });
});
