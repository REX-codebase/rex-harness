// Plate programs: what the real site scene does during each shot.
// A program is a pure function of local time u (seconds into the shot) that returns
//   chapter  which chapter's pose / choreography to show (website/js/scene/rig.js POSES)
//   p        progress 0..1 through that chapter (what scrolling drives on the site)
//   cam      optional camera overrides applied after the rig: dolly, yaw, lift, sx, sy, fovMul
//   events   {at, type, ...} one-shot events fired the first frame u >= at
// Everything here is a direction for the scene the site already ships; no scene code is changed.

const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, t) => a + (b - a) * t;
const easeOut = (x) => 1 - Math.pow(1 - clamp(x), 3);
const easeInOut = (x) => { x = clamp(x); return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2; };

export const CUSTODY_EVENTS = [
  { at: 1.0, type: 'propose', id: 1 },                        // arrives at the gate 2.3 s later
  { at: 4.6, type: 'decision', id: 1, decision: 'approve' },  // the human clicks Approve
  { at: 6.8, type: 'mode', mode: 'auto' },                    // ...then turns Auto on
  { at: 6.9, type: 'propose', id: 2 },
  { at: 9.85, type: 'decision', id: 2, decision: 'approve' }, // auto approves 650 ms after it arrives
];
export const LIMITS_START = 2.4; // u at which the real "Start a run" demo is clicked

export const PLATES = {
  // A distant light resolves into the entity.
  hook: (u, d) => ({
    chapter: 'hero', p: 0,
    cam: { dolly: lerp(3.9, 1.0, easeOut(u / (d - 0.2))), yaw: 0.35 * (1 - easeOut(u / d)), lift: lerp(3, 0, easeOut(u / d)) },
  }),
  // The hero shot, a slow orbit.
  title: (u, d) => ({
    chapter: 'hero', p: lerp(0.0, 0.5, u / d),
    cam: { dolly: lerp(1.04, 0.93, easeInOut(u / d)) },
  }),
  custody: (u, d) => ({ chapter: 'custody', p: lerp(0.08, 0.92, u / d), cam: { dolly: lerp(1.0, 0.9, easeInOut(u / d)) }, events: CUSTODY_EVENTS }),
  limits: (u, d) => ({
    chapter: 'limits', p: lerp(0.22, 0.82, u / d), cam: { dolly: lerp(1.04, 0.92, easeInOut(u / d)) },
    events: [{ at: LIMITS_START, type: 'click', sel: '#run-start' }],
  }),
  proof: (u, d) => ({ chapter: 'proof', p: clamp(u / d), cam: { dolly: lerp(1.0, 0.92, easeInOut(u / d)) } }),
  ultra: (u, d) => ({ chapter: 'ultra', p: clamp(u / d) }),
  models: (u, d) => ({ chapter: 'models', p: lerp(0.15, 0.85, u / d), cam: { dolly: lerp(1.0, 0.88, easeInOut(u / d)) } }),
  // "What's new": the entity centred behind the real interface screenshot.
  new: (u, d) => ({ chapter: 'hero', p: lerp(0.5, 0.75, u / d), cam: { dolly: lerp(1.5, 1.35, easeInOut(u / d)), sx: 0.02, sy: -0.02 } }),
  // Pull back for the call to action.
  cta: (u, d) => ({ chapter: 'hero', p: lerp(0.75, 1.0, u / d), cam: { dolly: lerp(1.35, 1.9, easeInOut(u / d)), sx: 0.26, sy: 0.06 } }),
};
