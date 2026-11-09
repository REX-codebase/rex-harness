// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach } from "vitest";
import { Ambient, indicatorBox, runPhase, trackGlow, useSlideIndicator } from "./Fx";
import { StateRail } from "./StateRail";

afterEach(cleanup);

describe("indicatorBox", () => {
  it("returns the active control's box", () => {
    expect(indicatorBox({ offsetLeft: 12, offsetTop: 2, offsetWidth: 64, offsetHeight: 30 })).toEqual({ x: 12, y: 2, w: 64, h: 30 });
  });
  it("is null without layout so callers keep the plain highlight", () => {
    expect(indicatorBox(null)).toBeNull();
    expect(indicatorBox({ offsetLeft: 0, offsetTop: 0, offsetWidth: 0, offsetHeight: 0 })).toBeNull();
  });
});

describe("runPhase", () => {
  it("maps every agent status to one phase", () => {
    expect(runPhase({ agentStatus: "running" })).toBe("working");
    expect(runPhase({ agentStatus: "planning" })).toBe("working");
    expect(runPhase({ agentStatus: "verifying" })).toBe("working");
    expect(runPhase({ agentStatus: "awaiting_approval" })).toBe("waiting");
    expect(runPhase({ agentStatus: "awaiting_plan" })).toBe("waiting");
    expect(runPhase({ agentStatus: "awaiting_answer" })).toBe("waiting");
    expect(runPhase({ agentStatus: "completed" })).toBe("done");
    for (const s of ["blocked", "denied", "cancelled", "failed"]) expect(runPhase({ agentStatus: s })).toBe("blocked");
  });
  it("never reports a failed Ultra ending as done", () => {
    expect(runPhase({ ultraActive: true, ultraTerminalKind: null })).toBe("working");
    expect(runPhase({ ultraActive: true, ultraTerminalKind: "promoted" })).toBe("done");
    for (const k of ["rejected", "builder_failed", "contract_failed", "provider_error", "cancelled"]) {
      expect(runPhase({ ultraActive: true, ultraTerminalKind: k })).toBe("blocked");
    }
  });
  it("maps installed-agent runs and the sample engine", () => {
    expect(runPhase({ installedStatus: "running" })).toBe("working");
    expect(runPhase({ installedStatus: "awaiting_review" })).toBe("waiting");
    expect(runPhase({ installedStatus: "completed" })).toBe("done");
    expect(runPhase({ installedStatus: "failed" })).toBe("blocked");
    expect(runPhase({ mockState: "verifying" })).toBe("working");
    expect(runPhase({ mockState: "done" })).toBe("done");
    expect(runPhase({ mockState: "blocked" })).toBe("blocked");
    expect(runPhase({ mockState: "idle" })).toBe("idle");
    expect(runPhase({})).toBe("idle");
  });
  it("lets a live agent run outrank a stale sample session", () => {
    expect(runPhase({ agentStatus: "running", mockState: "done" })).toBe("working");
  });
});

describe("trackGlow", () => {
  it("records the pointer relative to the nearest [data-glow] ancestor", () => {
    const { container } = render(
      <div data-glow data-testid="field"><span data-testid="inner">x</span></div>,
    );
    const field = container.querySelector<HTMLElement>("[data-glow]")!;
    field.getBoundingClientRect = () => ({ left: 100, top: 50, right: 500, bottom: 250, width: 400, height: 200, x: 100, y: 50, toJSON() {} });
    trackGlow(screen.getByTestId("inner"), 160, 95);
    expect(field.style.getPropertyValue("--mx")).toBe("60.0px");
    expect(field.style.getPropertyValue("--my")).toBe("45.0px");
  });
  it("ignores pointers outside any glow element", () => {
    const { container } = render(<div data-testid="plain">x</div>);
    expect(() => trackGlow(container.firstElementChild, 10, 10)).not.toThrow();
    expect((container.firstElementChild as HTMLElement).style.getPropertyValue("--mx")).toBe("");
  });
});

describe("useSlideIndicator", () => {
  function Tabs({ active }: { active: string }) {
    const ref = useRef<HTMLElement>(null);
    useSlideIndicator(ref, active, ".is-current");
    return (
      <nav ref={ref} data-testid="nav">
        <button className={active === "a" ? "is-current" : ""}>A</button>
        <button className={active === "b" ? "is-current" : ""}>B</button>
      </nav>
    );
  }
  it("without layout it leaves the plain per-button highlight in effect", () => {
    render(<Tabs active="a" />);
    // jsdom reports 0 sizes: no pill is placed, so CSS keeps the button fill.
    expect(screen.getByTestId("nav").hasAttribute("data-ind")).toBe(false);
  });
  it("places the pill from the active control's box once it has layout", () => {
    const boxes: Record<string, [number, number]> = { A: [4, 40], B: [48, 52] };
    const proto = HTMLElement.prototype;
    const defs = ["offsetLeft", "offsetWidth", "offsetTop", "offsetHeight"] as const;
    const saved = defs.map((k) => Object.getOwnPropertyDescriptor(proto, k));
    Object.defineProperty(proto, "offsetLeft", { configurable: true, get() { return boxes[(this as HTMLElement).textContent ?? ""]?.[0] ?? 0; } });
    Object.defineProperty(proto, "offsetWidth", { configurable: true, get() { return boxes[(this as HTMLElement).textContent ?? ""]?.[1] ?? 0; } });
    Object.defineProperty(proto, "offsetTop", { configurable: true, get() { return 2; } });
    Object.defineProperty(proto, "offsetHeight", { configurable: true, get() { return (this as HTMLElement).tagName === "BUTTON" ? 30 : 0; } });
    try {
      const { rerender } = render(<Tabs active="a" />);
      const nav = screen.getByTestId("nav");
      expect(nav.hasAttribute("data-ind")).toBe(true);
      expect(nav.style.getPropertyValue("--ind-x")).toBe("4px");
      expect(nav.style.getPropertyValue("--ind-w")).toBe("40px");
      rerender(<Tabs active="b" />);
      expect(nav.style.getPropertyValue("--ind-x")).toBe("48px");
      expect(nav.style.getPropertyValue("--ind-w")).toBe("52px");
    } finally {
      defs.forEach((k, i) => { if (saved[i]) Object.defineProperty(proto, k, saved[i]!); else delete (proto as unknown as Record<string, unknown>)[k]; });
    }
  });
});

describe("Ambient", () => {
  it("is decorative and hidden from assistive tech", () => {
    const { container } = render(<Ambient reduced={false} />);
    expect(container.querySelector(".ambient")?.getAttribute("aria-hidden")).toBe("true");
  });
  it("does not follow the pointer when motion is reduced", () => {
    const { container } = render(<Ambient reduced />);
    const halo = container.querySelector<HTMLElement>(".amb-halo")!;
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 300, clientY: 300 }));
    expect(halo.style.getPropertyValue("--px")).toBe("");
  });
});

describe("StateRail", () => {
  it("shows nothing when idle", () => {
    const { container } = render(<StateRail state="idle" />);
    expect(container.firstChild).toBeNull();
  });
  it("marks exactly one stage active and the earlier ones complete while working", () => {
    const { container } = render(<StateRail state="verifying" />);
    const stages = [...container.querySelectorAll<HTMLElement>(".rail-stage")].map((s) => s.dataset.st);
    expect(stages).toEqual(["complete", "active", "pending"]);
    expect(container.querySelectorAll('[aria-current="step"]')).toHaveLength(1);
    expect(screen.getByText("Verifying").getAttribute("aria-current")).toBe("step");
  });
  it("completes every stage when done, and a link only fills once its stage is reached", () => {
    const { container } = render(<StateRail state="done" />);
    expect([...container.querySelectorAll<HTMLElement>(".rail-stage")].map((s) => s.dataset.st)).toEqual(["complete", "complete", "complete"]);
    expect([...container.querySelectorAll<HTMLElement>(".rail-link")].map((l) => l.dataset.st)).toEqual(["complete", "complete"]);
  });
  it("never fills a link ahead of the active stage (activity, not invented progress)", () => {
    const { container } = render(<StateRail state="working" />);
    expect([...container.querySelectorAll<HTMLElement>(".rail-link")].map((l) => l.dataset.st)).toEqual(["active", "pending"]);
  });
  it("states why a run is blocked and stays polite for screen readers", () => {
    const { container } = render(<StateRail state="blocked" blockedReason="Needs a model key." />);
    expect(screen.getByText("Needs a model key.")).toBeTruthy();
    expect(container.querySelector('[aria-live="polite"]')).toBeTruthy();
  });
  it("keeps the decorative rail out of the accessibility tree", () => {
    const { container } = render(<StateRail state="working" />);
    expect([...container.querySelectorAll(".rail-node, .rail-link")].every((e) => e.getAttribute("aria-hidden") === "true")).toBe(true);
  });
});
