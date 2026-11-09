import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

// Visual-effects primitives for the motion layer (src/motion-ui.css).
//
// Everything here is presentation only: it never changes what the UI says
// about a run. Each effect is also gated by the shell's resolved motion
// preference (docs/motion.md): when `reduced` is true the JS loops below do
// not start, and the stylesheet drops the decorative layers.

export type IndicatorBox = { x: number; y: number; w: number; h: number };

// Where the sliding pill should sit: the active control's box inside its
// container. Null when the control has no layout (jsdom, display:none, not
// mounted yet) so callers fall back to the plain per-button highlight.
export function indicatorBox(el: { offsetLeft: number; offsetTop: number; offsetWidth: number; offsetHeight: number } | null): IndicatorBox | null {
  if (!el || el.offsetWidth <= 0 || el.offsetHeight <= 0) return null;
  return { x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight };
}

// One pill that glides between the options of a segmented control instead of
// each option flashing its own background. The container carries the pill as
// a ::before driven by CSS variables, so there is no extra DOM. Without
// layout the container never gets `data-ind` and the old per-button
// highlight stays in effect.
export function useSlideIndicator<T extends HTMLElement>(ref: RefObject<T>, activeKey: unknown, selector = ".is-current, .is-on") {
  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const place = () => {
      const box = indicatorBox(root.querySelector<HTMLElement>(selector));
      if (!box) {
        root.removeAttribute("data-ind");
        return;
      }
      root.style.setProperty("--ind-x", `${box.x}px`);
      root.style.setProperty("--ind-y", `${box.y}px`);
      root.style.setProperty("--ind-w", `${box.w}px`);
      root.style.setProperty("--ind-h", `${box.h}px`);
      root.setAttribute("data-ind", "");
    };
    place();
    // Transitions switch on only after the first placement has painted, so
    // the pill appears in position instead of flying in from the corner.
    const frame = window.requestAnimationFrame(() => root.setAttribute("data-ind-live", ""));
    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(place);
      observer.observe(root);
      root.querySelectorAll("button").forEach((b) => observer?.observe(b));
    }
    // Web fonts change label widths after first layout.
    document.fonts?.ready.then(place).catch(() => undefined);
    return () => {
      window.cancelAnimationFrame(frame);
      observer?.disconnect();
    };
  }, [ref, activeKey, selector]);
}

// Records the pointer inside the nearest [data-glow] element as --mx/--my so
// CSS can draw a light that follows the cursor across a field or card.
export function trackGlow(target: EventTarget | null, clientX: number, clientY: number): void {
  const el = (target as Element | null)?.closest?.("[data-glow]") as HTMLElement | null;
  if (!el) return;
  const r = el.getBoundingClientRect();
  el.style.setProperty("--mx", `${(clientX - r.left).toFixed(1)}px`);
  el.style.setProperty("--my", `${(clientY - r.top).toFixed(1)}px`);
}

// Fixed backdrop behind the whole window: a lit horizon, a faint ledger
// grid, a cursor halo that trails the pointer with a little lag, a grain to
// stop banding, and a one-shot bloom when a run completes. Its tint follows
// the run state through the shell's data-run attribute. Purely decorative.
export function Ambient({ reduced }: { reduced: boolean }) {
  const haloRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (reduced) return;
    const halo = haloRef.current;
    let tx = window.innerWidth / 2;
    let ty = window.innerHeight * 0.22;
    let cx = tx;
    let cy = ty;
    let frame = 0;
    const tick = () => {
      cx += (tx - cx) * 0.08;
      cy += (ty - cy) * 0.08;
      halo?.style.setProperty("--px", `${cx.toFixed(1)}px`);
      halo?.style.setProperty("--py", `${cy.toFixed(1)}px`);
      frame = Math.abs(tx - cx) > 0.4 || Math.abs(ty - cy) > 0.4 ? window.requestAnimationFrame(tick) : 0;
    };
    const onMove = (e: PointerEvent) => {
      tx = e.clientX;
      ty = e.clientY;
      trackGlow(e.target, e.clientX, e.clientY);
      if (!frame) frame = window.requestAnimationFrame(tick);
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.cancelAnimationFrame(frame);
    };
  }, [reduced]);
  return (
    <div className="ambient" aria-hidden="true">
      <span className="amb-cone" />
      <span className="amb-grid" />
      <span className="amb-halo" ref={haloRef} />
      <span className="amb-bloom" />
      <span className="amb-grain" />
    </div>
  );
}

// The shell's data-run value: one word for what the run is doing, for any
// engine, so the backdrop can tint to match. Never shown to the user.
export type RunPhase = "idle" | "working" | "waiting" | "done" | "blocked";

// Ultra reports a terminal object; only `promoted` is a success, every other
// ending (rejected, failed, cancelled) must not tint the backdrop as done.
export function runPhase(args: { agentStatus?: string | null; ultraActive?: boolean; ultraTerminalKind?: string | null; mockState?: string; installedStatus?: string | null }): RunPhase {
  const { agentStatus, ultraActive, ultraTerminalKind, mockState, installedStatus } = args;
  if (agentStatus) {
    if (agentStatus === "completed") return "done";
    if (agentStatus.startsWith("awaiting")) return "waiting";
    if (agentStatus === "planning" || agentStatus === "running" || agentStatus === "verifying") return "working";
    return "blocked";
  }
  if (installedStatus) {
    if (installedStatus === "running") return "working";
    if (installedStatus === "completed") return "done";
    if (installedStatus.startsWith("awaiting")) return "waiting";
    return "blocked";
  }
  if (ultraActive) return ultraTerminalKind ? (ultraTerminalKind === "promoted" ? "done" : "blocked") : "working";
  if (mockState === "working" || mockState === "verifying") return "working";
  if (mockState === "done") return "done";
  if (mockState === "blocked") return "blocked";
  return "idle";
}
