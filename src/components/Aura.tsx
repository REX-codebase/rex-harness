import type { CSSProperties } from "react";

// Soft light behind the top-left wordmark, so the mark's glow bleeds into the
// page. It rides on the motion layer's backdrop (<Ambient>, Fx.tsx) rather than
// replacing it: one blob takes the wordmark's own glow colour (silver-violet,
// champagne in Ultra) and one takes the run-state tint the backdrop uses.
// Purely decorative: no content, no pointer events, hidden from assistive tech.
export function AuraLayer() {
  return (
    <div className="aura" aria-hidden="true">
      <span className="aura-blob aura-a" />
      <span className="aura-blob aura-b" />
    </div>
  );
}

export type Origin = { x: number; y: number };

const IN_WORD = "ULTRA";
const OUT_WORD = "STANDARD";
const SLATS = 7;

// The Ultra tier change, staged as one shot:
//   a shockwave leaves the control that was pressed,
//   a run of slanted slats (the wordmark's own angle) shutters across the screen,
//   the tier name types itself in the gap, then everything clears.
// Entering is champagne and sweeps left to right; leaving is violet and
// sweeps back. The state flips at t=0, so the slats are what hides the shift.
// It is a pure overlay: it never blocks input and never claims a capability.
export function UltraTransition({ engaged, origin }: { engaged: boolean; origin: Origin | null }) {
  const word = engaged ? IN_WORD : OUT_WORD;
  const style = origin ? ({ "--ux": `${origin.x}px`, "--uy": `${origin.y}px` } as CSSProperties) : undefined;
  return (
    <div className={`ultra-transition ${engaged ? "is-in" : "is-out"} ${origin ? "" : "no-origin"}`} style={style} aria-hidden="true">
      <span className="ut-flash" />
      <span className="ut-ring" />
      <span className="ut-ring ut-ring-2" />
      <div className="ut-slats">
        {Array.from({ length: SLATS }, (_, k) => <i key={k} style={{ "--k": k } as CSSProperties} />)}
      </div>
      <div className="ut-word">
        <b>{word.split("").map((ch, k) => <span key={k} style={{ "--k": k } as CSSProperties}>{ch}</span>)}</b>
        <i />
        <small>{engaged ? "VERIFICATION TIER ENGAGED" : "VERIFICATION TIER OFFLINE"}</small>
      </div>
    </div>
  );
}
