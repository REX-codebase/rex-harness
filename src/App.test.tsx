// @vitest-environment jsdom
//
// The task screen's run lifecycle, driven through the real App with only the
// backend bridges replaced. These pin the behaviour that no component test
// reached: what Run does with no backend, that a run in progress cannot be
// started over, that a failed start names the provider that was selected, and
// that a finished live run has a way back to an empty composer.
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSnapshot } from "./data/agentTypes";

const bridge = vi.hoisted(() => ({
  available: false,
  custodyBegin: vi.fn(),
  agentSnapshot: vi.fn(),
  agentTeardown: vi.fn(),
  provider: "gemini",
}));

vi.mock("./data/liveRun", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./data/liveRun")>()),
  liveRunAvailable: () => Promise.resolve(bridge.available),
}));
vi.mock("./data/custodyRun", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./data/custodyRun")>()),
  custodyBegin: (...args: unknown[]) => bridge.custodyBegin(...args),
  selectedModel: () => ({ provider: bridge.provider }),
}));
vi.mock("./data/agentRun", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./data/agentRun")>()),
  agentSnapshot: (...args: unknown[]) => bridge.agentSnapshot(...args),
  agentTeardown: (...args: unknown[]) => bridge.agentTeardown(...args),
  canAllowForRun: () => Promise.resolve(false),
}));

import App from "./App";

function snapshot(partial: Partial<AgentSnapshot>): AgentSnapshot {
  return {
    id: "run-1",
    task: "do the thing",
    status: "running",
    terminal_reason: null,
    provider: "anthropic",
    model: "m",
    plan: [],
    step: 1,
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

const custodyView = (snap: AgentSnapshot) => ({
  grant_id: "grant-1234567890abcdef",
  task_id: "task-1",
  operator: "human",
  worker: "worker",
  phase: "active",
  snapshot: snap,
});

const taskBox = () => screen.getByLabelText("Task description") as HTMLTextAreaElement;
const runButton = () => screen.getByRole("button", { name: /^(Run task|Plan task|Run in progress)$/ }) as HTMLButtonElement;

async function typeTask(text: string) {
  fireEvent.change(taskBox(), { target: { value: text } });
}

beforeEach(() => {
  bridge.available = false;
  bridge.provider = "gemini";
  bridge.custodyBegin.mockReset();
  bridge.agentSnapshot.mockReset();
  bridge.agentTeardown.mockReset().mockResolvedValue({ ok: true });
  window.localStorage.clear();
  window.localStorage.setItem("rex-operator-mode", "human");
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(cleanup);

describe("with no backend connected", () => {
  it("keeps the composer when Run is pressed, and points at the setup steps", async () => {
    render(<App />);
    await screen.findByLabelText("Connect a backend");
    await typeTask("fix the flaky test");
    fireEvent.click(runButton());
    expect(screen.getByLabelText("Connect a backend").className).toContain("attn");
    // The hero composer used to settle away here, leaving nothing to type
    // into and a checklist that says "describe a task above".
    await act(async () => {
      await new Promise((r) => setTimeout(r, 700));
    });
    expect(screen.queryByLabelText("Task description")).not.toBeNull();
  });

  it("still offers the sample runs, labelled as samples, in the preview", async () => {
    render(<App />);
    await screen.findByLabelText("Connect a backend");
    expect(screen.getByText("Sample runs")).toBeTruthy();
  });
});

describe("with a backend connected", () => {
  beforeEach(() => {
    bridge.available = true;
  });

  it("does not list invented sample runs next to the person's real work", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
    expect(screen.queryByText("Sample runs")).toBeNull();
    expect(screen.queryByText(/Publish the billing webhook/)).toBeNull();
  });

  it("locks the composer while a run is going and never starts a second run over it", async () => {
    bridge.custodyBegin.mockResolvedValue(custodyView(snapshot({})));
    bridge.agentSnapshot.mockResolvedValue(snapshot({}));
    render(<App />);
    await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
    await typeTask("first task");
    fireEvent.click(runButton());
    await screen.findByLabelText("Autonomous run");

    expect(taskBox().disabled).toBe(true);
    expect(runButton().disabled).toBe(true);
    // Ctrl+Enter and a stray click are both refused.
    fireEvent.keyDown(taskBox(), { key: "Enter", ctrlKey: true });
    fireEvent.click(runButton());
    expect(bridge.custodyBegin).toHaveBeenCalledTimes(1);
  });

  it("shows that a start is under way and ignores a double click before the backend answers", async () => {
    let finish: (v: unknown) => void = () => undefined;
    bridge.custodyBegin.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    render(<App />);
    await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
    await typeTask("slow start");
    const button = runButton();
    fireEvent.click(button);
    fireEvent.click(button);
    expect(bridge.custodyBegin).toHaveBeenCalledTimes(1);
    await screen.findByText("Contacting the live model catalog…");
    await act(async () => finish(custodyView(snapshot({}))));
    await screen.findByLabelText("Autonomous run");
  });

  it("names the selected provider, not Gemini, when a run cannot start", async () => {
    bridge.provider = "openai";
    bridge.custodyBegin.mockRejectedValue("no API key saved for openai");
    render(<App />);
    await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
    await typeTask("anything");
    fireEvent.click(runButton());
    await screen.findByText(/no API key saved for openai/);
    expect(screen.getByText("openai/resolving…")).toBeTruthy();
    expect(screen.queryByText(/gemini/)).toBeNull();
    // A start that failed leaves nothing running, so the composer is free.
    expect(taskBox().disabled).toBe(false);
  });

  it("offers New task once a run has finished, and a fresh run replaces the old one", async () => {
    bridge.custodyBegin.mockResolvedValue(custodyView(snapshot({})));
    bridge.agentSnapshot.mockResolvedValue(
      snapshot({ status: "completed", terminal_reason: { kind: "completed" }, completion_summary: "All gates passed" }),
    );
    render(<App />);
    await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
    await typeTask("finish quickly");
    fireEvent.click(runButton());
    await screen.findByText("All gates passed", undefined, { timeout: 4000 });
    expect(screen.queryByRole("button", { name: "New task" })).not.toBeNull();
    expect(taskBox().disabled).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "New task" }));
    expect(screen.queryByLabelText("Autonomous run")).toBeNull();
    expect(screen.queryByRole("button", { name: "New task" })).toBeNull();
    expect(taskBox().value).toBe("");
    // The finished run's sandbox is released, not left behind.
    expect(bridge.agentTeardown).toHaveBeenCalledWith("run-1");
  });

  it("says so when the backend stops answering, and lets the person stop watching", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      bridge.custodyBegin.mockResolvedValue(custodyView(snapshot({})));
      bridge.agentSnapshot.mockRejectedValue(new Error("connection refused"));
      render(<App />);
      await waitFor(() => expect(screen.getByText("Backend connected")).toBeTruthy());
      await typeTask("long task");
      fireEvent.click(runButton());
      await screen.findByLabelText("Autonomous run");
      expect(screen.queryByRole("alert")).toBeNull();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(900 * 9);
      });
      const notice = await screen.findByText(/Lost contact with the backend/);
      expect(notice).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Stop watching" }));
      expect(screen.queryByLabelText("Autonomous run")).toBeNull();
      expect(taskBox().disabled).toBe(false);
      // Detaching must not tear the run down: teardown cancels a live run.
      expect(bridge.agentTeardown).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
