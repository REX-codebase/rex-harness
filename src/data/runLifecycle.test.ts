import { describe, expect, it } from "vitest";
import {
  FAILED_RUN_ID,
  errorText,
  failedInstalledRun,
  failedRunSnapshot,
  runFinished,
  runInProgress,
  type RunStates,
} from "./runLifecycle";
import { phaseOf, terminalLabel, type AgentSnapshot } from "./agentTypes";
import type { UltraSnapshot } from "./ultraRun";
import type { InstalledRunSnapshot } from "./installedAgentRun";

const agent = (partial: Partial<AgentSnapshot> = {}): AgentSnapshot => ({
  ...failedRunSnapshot("t", "x", { provider: "anthropic" }),
  id: "r1",
  status: "running",
  terminal_reason: null,
  error: null,
  ...partial,
});
const ultra = (terminal: UltraSnapshot["terminal"]) => ({ id: "u1", terminal }) as unknown as UltraSnapshot;
const installed = (status: InstalledRunSnapshot["status"]) =>
  ({ id: "i1", status }) as unknown as InstalledRunSnapshot;
const idle: RunStates = { starting: false, agent: null, ultra: null, installed: null };

describe("failedRunSnapshot", () => {
  it("names the provider the person selected, never a default", () => {
    expect(failedRunSnapshot("t", "boom", { provider: "openai" }).provider).toBe("openai");
    expect(failedRunSnapshot("t", "boom", { provider: "groq" }).provider).toBe("groq");
  });

  it("is a finished run that carries the reason, so it never reads as in progress", () => {
    const snap = failedRunSnapshot("t", "no API key saved", { provider: "openai" });
    expect(snap.id).toBe(FAILED_RUN_ID);
    expect(snap.error).toBe("no API key saved");
    expect(snap.terminal_reason).toEqual({ kind: "provider_error", detail: "no API key saved" });
    expect(phaseOf(snap)).toBe("stopped");
    expect(runInProgress({ ...idle, agent: snap })).toBe(false);
  });

  it("reports a refusal by REX as blocked rather than as a provider failure", () => {
    const snap = failedRunSnapshot("t", "the gate would not open", { provider: "openai", kind: "blocked" });
    expect(snap.status).toBe("blocked");
    expect(terminalLabel(snap.terminal_reason!)).toBe("Blocked - the gate would not open");
  });
});

describe("failedInstalledRun", () => {
  it("is a finished, unpromotable record that carries the reason", () => {
    const run = failedInstalledRun("gemini-cli" as never, "t", "vendor CLI not found", 5);
    expect(run.id).toBe(FAILED_RUN_ID);
    expect(run.status).toBe("failed");
    expect(run.error).toBe("vendor CLI not found");
    expect(run.promotion).toBe("not_required");
    expect(run.created_at_ms).toBe(5);
    expect(runInProgress({ ...idle, installed: run })).toBe(false);
  });
});

describe("runInProgress", () => {
  it("is false with nothing on screen", () => {
    expect(runInProgress(idle)).toBe(false);
    expect(runFinished(idle)).toBe(false);
  });

  it("is true while a run is being started, before any snapshot exists", () => {
    expect(runInProgress({ ...idle, starting: true })).toBe(true);
  });

  it("holds for every non-terminal custody status, including waiting on the person", () => {
    for (const status of ["planning", "running", "awaiting_approval", "awaiting_plan", "awaiting_answer", "verifying"] as const) {
      expect(runInProgress({ ...idle, agent: agent({ status }) })).toBe(true);
    }
    expect(runInProgress({ ...idle, agent: agent({ status: "completed", terminal_reason: { kind: "completed" } }) })).toBe(false);
    expect(runInProgress({ ...idle, agent: agent({ status: "cancelled", terminal_reason: { kind: "cancelled" } }) })).toBe(false);
  });

  it("follows the Ultra terminal outcome", () => {
    expect(runInProgress({ ...idle, ultra: ultra(null) })).toBe(true);
    expect(runInProgress({ ...idle, ultra: ultra({ kind: "promoted" }) })).toBe(false);
    expect(runInProgress({ ...idle, ultra: ultra({ kind: "rejected", reasons: [] }) })).toBe(false);
  });

  it("keeps an installed agent's staged result open until it is promoted or discarded", () => {
    expect(runInProgress({ ...idle, installed: installed("running") })).toBe(true);
    expect(runInProgress({ ...idle, installed: installed("awaiting_review") })).toBe(true);
    for (const status of ["completed", "failed", "cancelled"] as const) {
      expect(runInProgress({ ...idle, installed: installed(status) })).toBe(false);
    }
  });
});

describe("runFinished", () => {
  it("is true only when a result is on screen and nothing is still running", () => {
    const done = agent({ status: "completed", terminal_reason: { kind: "completed" } });
    expect(runFinished({ ...idle, agent: done })).toBe(true);
    expect(runFinished({ ...idle, agent: agent() })).toBe(false);
    expect(runFinished({ ...idle, agent: done, starting: true })).toBe(false);
  });
});

describe("errorText", () => {
  it("reads the message of an Error, a string rejection (what Tauri sends) and other values", () => {
    expect(errorText(new Error("bad key"))).toBe("bad key");
    expect(errorText("provider HTTP 401")).toBe("provider HTTP 401");
    expect(errorText({ code: 7 })).toBe('{"code":7}');
    expect(errorText(undefined)).toBe("undefined");
  });
});
