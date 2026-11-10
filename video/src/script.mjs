// The film's timeline. One source of truth, imported by the plate renderer (Node), the
// compositor page (browser) and the cue sheet for the audio.
//
// The film is 24 fps, in a 2.39:1 frame (1920 x 804 picture inside a 1920 x 1080 canvas,
// with the black bars carrying the subtitles).

export const FPS = 24;
export const W = 1920;
export const H = 1080;
export const PH = 804;                 // picture height
export const BAR = (H - PH) / 2;       // 138 px letterbox bars

// plate: where the footage comes from.
//   new plates are rendered by tools/render-plates.mjs at 1920 x 804, 24 fps (src/plates.mjs)
//   legacy plates (legacy: true) were rendered at 1920 x 1080, 30 fps; they are cropped to the
//   2.39:1 window and resampled by time. u0 is where in the plate the shot starts.
export const SHOTS = [
  { id: 'open',    t0: 0.0,  t1: 6.0,  plate: { dir: 'open' } },
  { id: 'reveal',  t0: 6.0,  t1: 13.0, plate: { dir: 'reveal' } },
  { id: 'tension', t0: 13.0, t1: 19.0, plate: { dir: 'tension' } },
  { id: 'harness', t0: 19.0, t1: 29.0, plate: { dir: 'harness' } },
  { id: 'limits',  t0: 29.0, t1: 38.0, plate: { dir: 'limits', legacy: true, u0: 1.4 } },
  { id: 'gate',    t0: 38.0, t1: 44.5, plate: { dir: 'custody', legacy: true, u0: 0.4 } },
  { id: 'proof',   t0: 44.5, t1: 54.5, plate: { dir: 'proof', legacy: true, u0: 0.4 } },
  { id: 'ultra',   t0: 54.5, t1: 62.9, plate: { dir: 'ultra', legacy: true, u0: 0.0 } },
  { id: 'models',  t0: 62.9, t1: 67.5, plate: { dir: 'models', legacy: true, u0: 0.0 } },
  { id: 'finale',  t0: 67.5, t1: 77.5, plate: { dir: 'finale' } },
];
export const DURATION = SHOTS[SHOTS.length - 1].t1;
export const TOTAL_FRAMES = Math.round(DURATION * FPS);

export const shotAt = (t) => SHOTS.find((s) => t >= s.t0 && t < s.t1) || SHOTS[SHOTS.length - 1];
export const shotFrames = (s) => Math.round((s.t1 - s.t0) * FPS);
// the plate frame to show at film time t
export function plateFrame(shot, t) {
  const u = t - shot.t0;
  if (shot.plate.legacy) return Math.round((shot.plate.u0 + u) * 30);
  return Math.min(shotFrames(shot) - 1, Math.max(0, Math.round(u * FPS)));
}
