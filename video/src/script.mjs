// The film's timeline. One source of truth, imported by the plate renderer (Node),
// the compositor page (browser) and the audio synth (via `node tools/print-cues.mjs`).

export const FPS = 30;
export const W = 1920;
export const H = 1080;

// Each shot is one continuous take of the site's real WebGL scene (see src/plates.mjs).
export const SHOTS = [
  { id: 'hook',    t0: 0.0,  t1: 5.6 },
  { id: 'title',   t0: 5.6,  t1: 11.6 },
  { id: 'custody', t0: 11.6, t1: 23.4 },
  { id: 'limits',  t0: 23.4, t1: 34.0 },
  { id: 'proof',   t0: 34.0, t1: 44.6 },
  { id: 'ultra',   t0: 44.6, t1: 53.0 },
  { id: 'models',  t0: 53.0, t1: 57.6 },
  { id: 'new',     t0: 57.6, t1: 62.6 },
  { id: 'cta',     t0: 62.6, t1: 71.0 },
];
export const DURATION = SHOTS[SHOTS.length - 1].t1;
export const TOTAL_FRAMES = Math.round(DURATION * FPS);

export const shotAt = (t) => SHOTS.find((s) => t >= s.t0 && t < s.t1) || SHOTS[SHOTS.length - 1];
export const shotFrames = (s) => Math.round((s.t1 - s.t0) * FPS);
