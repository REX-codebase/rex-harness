import { describe, expect, it } from "vitest";
import {
  humanizeProviderDetail,
  formatClock,
  formatTokens,
  phaseOf,
  terminalActive,
  terminalLabel,
  type AgentSnapshot,
} from "./agentTypes";

function snap(partial: Partial<AgentSnapshot>): AgentSnapshot {
  return {
    id: "r1",
    task: "t",
    status: "running",
    terminal_reason: null,
    provider: "gemini",
    model: "m",
    plan: [],
    step: 0,
    max_steps: 24,
    tool_calls: 0,
    max_tool_calls: 80,
    tokens_used: 0,
    max_tokens: 250000,
    elapsed_ms: 0,
    max_wall_ms: 1200000,
    pending_approval: null,
    events: [],
    preview: null,
    completion_summary: null,
    error: null,
    ...partial,
  };
}

describe("agent snapshot view model", () => {
  it("maps statuses to phases truthfully", () => {
    expect(phaseOf(null)).toBe("working");
    expect(phaseOf(snap({ status: "planning" }))).toBe("working");
    expect(phaseOf(snap({ status: "running" }))).toBe("working");
    expect(phaseOf(snap({ status: "awaiting_approval" }))).toBe("approval");
    expect(phaseOf(snap({ status: "awaiting_answer" }))).toBe("question");
    expect(phaseOf(snap({ status: "verifying" }))).toBe("verifying");
    expect(phaseOf(snap({ status: "completed" }))).toBe("completed");
    expect(phaseOf(snap({ status: "blocked" }))).toBe("stopped");
    expect(phaseOf(snap({ status: "failed" }))).toBe("stopped");
    expect(phaseOf(snap({ status: "denied" }))).toBe("stopped");
    expect(phaseOf(snap({ status: "cancelled" }))).toBe("stopped");
  });

  it("detects terminal state only from a real terminal reason", () => {
    expect(terminalActive(snap({}))).toBe(false);
    expect(terminalActive(snap({ terminal_reason: { kind: "completed" } }))).toBe(true);
  });

  it("renders truthful terminal labels", () => {
    expect(terminalLabel({ kind: "completed" })).toBe("Completed");
    expect(terminalLabel({ kind: "budget_steps", max_steps: 24 })).toContain("24");
    expect(terminalLabel({ kind: "repeated_failure", tool: "read_file" })).toContain("read file");
    expect(terminalLabel({ kind: "provider_error", detail: "HTTP 401" })).toContain("401");
    expect(terminalLabel({ kind: "denied" })).toContain("denied");
  });

  it("formats clocks and token counts compactly", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(65_000)).toBe("1:05");
    expect(formatClock(20 * 60 * 1000)).toBe("20:00");
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(250_000)).toBe("250.0k");
  });
});

describe("humanizeProviderDetail", () => {
  const rec = (status: number, extra: Record<string, unknown> = {}) =>
    `provider HTTP ${status}: ${JSON.stringify({ http_status: status, ...extra, redacted: true })}`;

  it("explains a rate limit and what to do, with the provider's own wait", () => {
    const text = humanizeProviderDetail(rec(429, { retry_after_secs: 33.8, provider_error_code: "rate_limit_exceeded" }));
    expect(text).toContain("rate limit");
    expect(text).toContain("retry in about 34 s");
    expect(text).toContain("lean prompt");
    expect(text).not.toContain("{");
  });

  it("scales the wait to minutes and hours", () => {
    expect(humanizeProviderDetail(rec(429, { retry_after_secs: 120 }))).toContain("about 2 min");
    expect(humanizeProviderDetail(rec(429, { retry_after_secs: 10920 }))).toContain("about 3 h");
    expect(humanizeProviderDetail(rec(429))).not.toContain("retry in");
  });

  it("names the likely fix for auth and missing-model errors", () => {
    expect(humanizeProviderDetail(rec(401))).toContain("rejected the API key");
    expect(humanizeProviderDetail(rec(403))).toContain("HTTP 403");
    expect(humanizeProviderDetail(rec(404))).toContain("Pick another model");
  });

  it("keeps the code on other errors and never drops unknown text", () => {
    expect(humanizeProviderDetail(rec(500, { provider_error_code: "overloaded" }))).toContain("overloaded");
    expect(humanizeProviderDetail("provider transport failed: io error")).toBe("provider transport failed: io error");
    expect(humanizeProviderDetail("provider HTTP 429: not json {")).toBe("provider HTTP 429: not json {");
  });

  it("is what the terminal label shows for a provider error", () => {
    const label = terminalLabel({ kind: "provider_error", detail: rec(429, { retry_after_secs: 5 }) });
    expect(label.startsWith("Failed - ")).toBe(true);
    expect(label).toContain("rate limit");
  });
});
