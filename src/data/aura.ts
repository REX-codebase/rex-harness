import { useEffect, type RefObject } from "react";

// Surfaces that catch the pointer's light. Kept to the main working surfaces
// so the aura reads as one idea, not a hover effect on everything.
export const GLOW_SELECTOR = ".task-field,.followup-field,.tool-approval,.plan-approval,.setup-panel";

export type Box = { left: number; top: number; width: number; height: number };

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

// Pointer position inside a box, clamped to its edges so the light never
// slides off the surface it belongs to.
export function localPointer(box: Box, x: number, y: number): { x: number; y: number } {
  return { x: clamp(x - box.left, 0, box.width), y: clamp(y - box.top, 0, box.height) };
}

// Where the Ultra shockwave starts: the centre of the control that was
// activated. A keyboard "click" reports 0,0 for its pointer, so the control's
// own box is the only honest origin.
export function originOf(box: Box): { x: number; y: number } {
  return { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2) };
}

// Feeds the aura two things as CSS custom properties, never React state:
//   --mx/--my/--mo on the shell: the soft light that trails the cursor.
//   --px/--py on the surface under the cursor: the spotlight inside a field.
// Writes are batched to one per animation frame. Touch is ignored: a finger
// has no hover, and dragging would leave a stale glow behind.
export function useAuraPointer(shell: RefObject<HTMLElement>): void {
  useEffect(() => {
    const root = shell.current;
    if (!root) return;
    let frame = 0;
    let last: PointerEvent | null = null;
    const flush = () => {
      frame = 0;
      const e = last;
      if (!e) return;
      root.style.setProperty("--mx", `${e.clientX}px`);
      root.style.setProperty("--my", `${e.clientY}px`);
      root.style.setProperty("--mo", "1");
      const target = e.target instanceof Element ? e.target.closest<HTMLElement>(GLOW_SELECTOR) : null;
      if (target) {
        const p = localPointer(target.getBoundingClientRect(), e.clientX, e.clientY);
        target.style.setProperty("--px", `${p.x}px`);
        target.style.setProperty("--py", `${p.y}px`);
      }
    };
    const onMove = (e: PointerEvent) => {
      if (e.pointerType === "touch") return;
      last = e;
      if (!frame) frame = window.requestAnimationFrame(flush);
    };
    const onLeave = () => root.style.setProperty("--mo", "0");
    root.addEventListener("pointermove", onMove, { passive: true });
    root.addEventListener("pointerleave", onLeave);
    return () => {
      root.removeEventListener("pointermove", onMove);
      root.removeEventListener("pointerleave", onLeave);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [shell]);
}
