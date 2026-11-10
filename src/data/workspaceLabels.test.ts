import { describe, expect, it } from "vitest";
import { folderName, runSandboxLabel } from "./workspaceLabels";
import { languageFor } from "./language";

describe("run sandbox labels", () => {
  it("turns REX's hex-millisecond folder name into a date", () => {
    // 2026-11-10 in hex milliseconds
    const ms = Date.UTC(2026, 10, 10, 12, 0, 0);
    const label = runSandboxLabel(`task-ui-${ms.toString(16)}`, "en-US");
    expect(label).toMatch(/^Run · Nov 10/);
  });
  it("leaves any other name, and implausible timestamps, alone", () => {
    expect(runSandboxLabel("my-project")).toBe("my-project");
    expect(runSandboxLabel("task-ui-1")).toBe("task-ui-1");
    expect(runSandboxLabel("task-ui-zzz")).toBe("task-ui-zzz");
  });
  it("finds a folder's name with either kind of slash", () => {
    expect(folderName("/home/a/proj")).toBe("proj");
    expect(folderName("C:\\Users\\a\\proj\\")).toBe("proj");
    expect(folderName("")).toBe("");
  });
});

describe("editor language", () => {
  it("picks a language from the extension or the file name", () => {
    expect(languageFor("src/App.tsx")).toBe("typescript");
    expect(languageFor("main.rs")).toBe("rust");
    expect(languageFor("package.json")).toBe("json");
    expect(languageFor("Dockerfile")).toBe("dockerfile");
    expect(languageFor("deploy/run.SH")).toBe("shell");
    expect(languageFor("docs\\README.md")).toBe("markdown");
    expect(languageFor("Cargo.toml")).toBe("ini");
  });
  it("falls back to plain text rather than guessing", () => {
    expect(languageFor("LICENSE")).toBe("plaintext");
    expect(languageFor("notes.unknownext")).toBe("plaintext");
    expect(languageFor(".hidden")).toBe("plaintext");
  });
});
