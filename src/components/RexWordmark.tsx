import { useState, type CSSProperties } from "react";

// The REX mark, set as live type instead of a traced bitmap. Three glyphs
// share one metallic fill (bright upper half, cooler lower half - the same
// hard split as the original logo) and one aura: a breathing halo, a drop
// glow, and a sheen that crosses the letters. The glyphs type in one at a
// time with a caret that hops along, then the caret blinks out.
//
// The wordmark is real text. Screen readers get "REX" from the wrapper's
// label; the glyph spans and the sheen copy are decoration.
//
// Ultra does not remount the wordmark: its colours are registered custom
// properties that glide from silver to champagne. Only the glyph run is keyed
// to Ultra so it types itself again, timed to land as the transition clears.

export const WORDMARK = "REX";
export const FIRST_TYPE_DELAY_MS = 140;
export const RETYPE_DELAY_MS = 560;
export const TYPE_STEP_MS = 120;

type Replay = { ultra: boolean; run: number };

// `run` only changes when `ultra` flips, so the delay below is stable for the
// life of each typed run. Changing animation-delay under a finished animation
// would snap the letters back to their hidden first frame.
export function useReplay(ultra: boolean): Replay {
  const [state, setState] = useState<Replay>({ ultra, run: 0 });
  if (state.ultra !== ultra) {
    const next = { ultra, run: state.run + 1 };
    setState(next);
    return next;
  }
  return state;
}

export function RexWordmark({ ultra = false }: { ultra?: boolean }) {
  const { run } = useReplay(ultra);
  const delay = run === 0 ? FIRST_TYPE_DELAY_MS : RETYPE_DELAY_MS;
  const letters = WORDMARK.split("");
  const typedBy = delay + letters.length * TYPE_STEP_MS;
  const vars = { "--type-delay": `${delay}ms`, "--type-step": `${TYPE_STEP_MS}ms`, "--typed-by": `${typedBy}ms` } as CSSProperties;
  return (
    <span className={`rex-wordmark ${ultra ? "is-ultra" : ""}`} role="img" aria-label={WORDMARK}>
      <span className="rex-halo" aria-hidden="true" />
      <span className="rex-glyphs" key={run} data-text={WORDMARK} style={vars} aria-hidden="true">
        {letters.map((ch, i) => (
          <span key={i} className={`rex-glyph ${i === letters.length - 1 ? "is-last" : ""}`} style={{ "--i": i } as CSSProperties}>{ch}</span>
        ))}
      </span>
      <span className="rex-rail" key={`rail-${run}`} style={vars} aria-hidden="true" />
    </span>
  );
}

// "Harness" typed after the mark, so the lockup reads as one line being written.
export function BrandName({ ultra = false }: { ultra?: boolean }) {
  const { run } = useReplay(ultra);
  const delay = (run === 0 ? FIRST_TYPE_DELAY_MS : RETYPE_DELAY_MS) + WORDMARK.length * TYPE_STEP_MS;
  return (
    <span className="brand-name truncate text-sm font-medium tracking-[-0.01em] text-text" key={run} style={{ "--name-delay": `${delay}ms` } as CSSProperties}>Harness</span>
  );
}
