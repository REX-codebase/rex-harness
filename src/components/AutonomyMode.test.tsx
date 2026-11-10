// @vitest-environment jsdom
//
// Autonomous mode in the composer: the Auto segment, the one-time confirmation,
// and the rule that it is only offered where the run type honours it.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ModelStatus", () => ({ ModelStatus: () => null }));

import { Composer } from "./Composer";
import { ActivityTimeline } from "./ActivityTimeline";
import { AgentRunView } from "./AgentRunView";
import { AUTONOMY_TERMS, autonomyAcknowledged, autonomyAppliesTo, resetAutonomyAcknowledgement } from "../data/autonomy";
import { resolveExecutionPath } from "../data/executionPath";
import type { AgentSnapshot } from "../data/agentTypes";

const custody = { kind: "custody", label: "Custody run", detail: "" } as never;
const installed = { kind: "installed", label: "Installed agent", detail: "" } as never;

function renderComposer(extra: Record<string, unknown> = {}) {
  const props = {
    setPlanMode: vi.fn(),
    setAutonomous: vi.fn(),
    ...extra,
  };
  const view = render(
    <Composer
      task="t"
      setTask={() => {}}
      state="idle"
      onRun={() => {}}
      planMode={false}
      fableGate={false}
      setFableGate={() => {}}
      executionPath={custody}
      {...props}
    />,
  );
  return { ...view, props };
}

beforeEach(() => {
  resetAutonomyAcknowledgement();
});
afterEach(cleanup);

describe("the Auto segment", () => {
  it("sits beside Build and Plan as one exclusive control", () => {
    renderComposer();
    expect(screen.getAllByRole("radio").map((r) => r.textContent)).toEqual(["Build", "Plan", "Auto"]);
    expect(screen.getByRole("radio", { name: "Build" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("radio", { name: "Auto" }).getAttribute("aria-checked")).toBe("false");
  });

  it("is not offered at all when the caller has no autonomous switch", () => {
    renderComposer({ setAutonomous: undefined });
    expect(screen.queryByRole("radio", { name: "Auto" })).toBeNull();
  });

  it("asks before turning approvals off, with Cancel focused so a stray key cannot confirm", () => {
    const { props } = renderComposer();
    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    const dialog = screen.getByRole("alertdialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByText("Let REX work without asking?")).toBeTruthy();
    expect(document.activeElement?.textContent).toBe("Keep asking me");
    // nothing changed yet
    expect(props.setAutonomous).not.toHaveBeenCalled();
    // every term the person agrees to is on screen
    for (const t of AUTONOMY_TERMS) expect(screen.getByText(t.heading)).toBeTruthy();
  });

  it("stays off when the person cancels, by button, Escape or clicking outside", () => {
    const { props } = renderComposer();
    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep asking me" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(screen.queryByRole("alertdialog")).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    fireEvent.mouseDown(document.querySelector(".autonomy-overlay")!);
    expect(screen.queryByRole("alertdialog")).toBeNull();

    expect(props.setAutonomous).not.toHaveBeenCalled();
    expect(autonomyAcknowledged()).toBe(false);
  });

  it("turns on after confirming, drops Plan, and only asks once per session", () => {
    const { props } = renderComposer();
    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    fireEvent.click(screen.getByRole("button", { name: "Turn on autonomous mode" }));
    expect(props.setAutonomous).toHaveBeenCalledWith(true);
    expect(props.setPlanMode).toHaveBeenCalledWith(false);
    expect(autonomyAcknowledged()).toBe(true);
    expect(screen.queryByRole("alertdialog")).toBeNull();

    props.setAutonomous.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "Auto" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(props.setAutonomous).toHaveBeenCalledWith(true);
  });

  it("is switched off again by choosing Build or Plan", () => {
    const { props } = renderComposer({ autonomous: true });
    expect(screen.getByRole("radio", { name: "Auto" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByRole("radio", { name: "Plan" }));
    expect(props.setAutonomous).toHaveBeenCalledWith(false);
    expect(props.setPlanMode).toHaveBeenCalledWith(true);
    fireEvent.click(screen.getByRole("radio", { name: "Build" }));
    expect(props.setAutonomous).toHaveBeenLastCalledWith(false);
    expect(props.setPlanMode).toHaveBeenLastCalledWith(false);
  });

  it("says plainly what Run will do while it is on", () => {
    renderComposer({ autonomous: true });
    expect(screen.getByRole("button", { name: "Run task autonomously" })).toBeTruthy();
    expect(screen.getByText(/Autonomous: no approval prompts/)).toBeTruthy();
    expect((screen.getByLabelText("Task description") as HTMLTextAreaElement).placeholder).toMatch(/will not stop to ask/);
  });

  it("is disabled, and not claimed, for a run type that has its own gates", () => {
    renderComposer({ executionPath: installed, autonomous: true });
    const auto = screen.getByRole("radio", { name: "Auto" }) as HTMLButtonElement;
    expect(auto.disabled).toBe(true);
    expect(auto.title).toMatch(/custody loop/);
    // the stale flag does not make the screen say "autonomous"
    expect(auto.getAttribute("aria-checked")).toBe("false");
    expect(screen.queryByText(/Autonomous: no approval prompts/)).toBeNull();
    expect(screen.getByRole("button", { name: "Run task" })).toBeTruthy();
  });

  it("is locked while a run is going", () => {
    renderComposer({ locked: true });
    expect((screen.getByRole("radio", { name: "Auto" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("autonomy rules", () => {
  it("applies only to the custody path", () => {
    expect(autonomyAppliesTo("custody")).toBe(true);
    for (const kind of ["ultra", "installed", "delegate", "tour", "unavailable"] as const) {
      expect(autonomyAppliesTo(kind)).toBe(false);
    }
  });

  it("is named in the execution path only where it is honoured", () => {
    const base = { operatorMode: "human" as const, ultra: false, installedAgentId: null, installedAgentLabel: null, liveCapable: true, tourMode: false };
    expect(resolveExecutionPath({ ...base, autonomous: true }).label).toBe("Autonomous run");
    expect(resolveExecutionPath({ ...base }).label).toBe("Custody run");
    const withInstalled = resolveExecutionPath({ ...base, autonomous: true, installedAgentId: "codex", installedAgentLabel: "Codex" });
    expect(withInstalled.kind).toBe("installed");
    expect(withInstalled.label).not.toMatch(/Autonomous/);
  });

  it("states the limits that stay on, in the words the person agrees to", () => {
    const text = AUTONOMY_TERMS.map((t) => `${t.heading} ${t.body}`).join(" ");
    expect(text).toMatch(/hard limits stay on/i);
    expect(text).toMatch(/budgets/);
    expect(text).toMatch(/Cancel/);
    expect(text).toMatch(/MCP/);
  });
});

describe("autonomous runs on screen", () => {
  const snap = (over: Partial<AgentSnapshot>): AgentSnapshot => ({
    id: "r1",
    task: "t",
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
    ...over,
  });

  it("lists each action that ran without asking", () => {
    render(
      <ActivityTimeline
        events={[
          { state: "auto_approved", call_id: "c1", command: "Write seed.txt" },
          { state: "auto_approved", call_id: "c2", command: "cp seed.txt copy.txt" },
        ]}
      />,
    );
    expect(screen.getByText(/Ran without asking · Write seed.txt/)).toBeTruthy();
    expect(screen.getByText(/Ran without asking · cp seed.txt copy.txt/)).toBeTruthy();
  });

  it("shows an Autonomous line with the running count, and nothing for a normal run", () => {
    const { rerender } = render(
      <AgentRunView run={snap({ autonomous: true, auto_approved: 3 })} deciding={false} cancelling={false} onDecide={() => {}} onCancel={() => {}} />,
    );
    const note = screen.getByRole("note");
    expect(note.textContent).toMatch(/Autonomous/);
    expect(note.textContent).toMatch(/3 actions ran without asking/);
    expect(note.textContent).toMatch(/Cancel stops it/);
    // Cancel is still offered during an autonomous run
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();

    rerender(<AgentRunView run={snap({ autonomous: true, auto_approved: 1 })} deciding={false} cancelling={false} onDecide={() => {}} onCancel={() => {}} />);
    expect(screen.getByRole("note").textContent).toMatch(/1 action ran without asking/);

    rerender(<AgentRunView run={snap({})} deciding={false} cancelling={false} onDecide={() => {}} onCancel={() => {}} />);
    expect(screen.queryByText(/ran without asking/)).toBeNull();
  });
});
