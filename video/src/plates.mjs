// Plate programs for the film: what the website's real WebGL scene does during each new shot.
// A program is a pure function of local time u (seconds into the shot) returning
//   chapter  which chapter's pose / choreography to show (website/js/scene/rig.js POSES)
//   p        progress 0..1 through that chapter (what scrolling drives on the site)
//   cam      camera overrides applied after the rig: dolly, yaw, lift, sx, sy, fovMul, shake
//   rings    [4] per-ring { visible, scale, rot:[x,y,z] } (the harness)
//   pulse    core brightness surge, set every frame (the core "flaring")
//   events   {at, type, ...} one-shot events fired the first frame u >= at
// Nothing in the site's scene code is changed; these are directions for it.

const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, t) => a + (b - a) * t;
const easeOut = (x) => 1 - Math.pow(1 - clamp(x), 3);
const easeIn = (x) => Math.pow(clamp(x), 2.2);
const easeInOut = (x) => { x = clamp(x); return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2; };
const decay = (u, at, tau) => (u >= at ? Math.exp(-(u - at) / tau) : 0);

// ── timings shared with the compositor and the audio ────────────────────────
// local seconds inside the 'harness' shot at which each ring locks home
export const RING_LOCKS = [3.7, 4.9, 6.1, 7.3];
export const RING_FLIGHT = 1.0; // seconds a ring spends flying in before it locks
// local seconds inside 'reveal' and 'tension' at which the unbound core flares
export const REVEAL_FLARES = [{ at: 0.55, a: 0.9 }, { at: 3.1, a: 1.2 }, { at: 5.5, a: 2.2 }];
export const TENSION_FLARES = [{ at: 0.9, a: 0.7 }, { at: 1.5, a: 0.5 }, { at: 2.0, a: 0.9 }, { at: 3.6, a: 0.8 }, { at: 4.2, a: 1.1 }, { at: 4.55, a: 1.6 }, { at: 5.2, a: 2.4 }];

// the legacy footage (limits, gate, proof, ultra, models) was rendered with these
export const CUSTODY_EVENTS = [
  { at: 1.0, type: 'propose', id: 1 },
  { at: 4.6, type: 'decision', id: 1, decision: 'approve' },
  { at: 6.8, type: 'mode', mode: 'auto' },
  { at: 6.9, type: 'propose', id: 2 },
  { at: 9.85, type: 'decision', id: 2, decision: 'approve' },
];
export const LIMITS_START = 2.4;

const flares = (list, u) => list.reduce((s, f) => s + f.a * decay(u, f.at, 0.32), 0);
const HIDDEN = [0, 1, 2, 3].map(() => ({ visible: false, scale: 3, rot: [0, 0, 0] }));
const ringSpin = [[1.7, 0.9, -1.1], [-1.3, 1.6, 0.8], [0.9, -1.8, 1.4], [-1.6, -0.7, -1.5]];

function ringsAt(u, locks = RING_LOCKS) {
  return locks.map((lock, i) => {
    const start = lock - RING_FLIGHT;
    if (u < start) return { visible: false, scale: 1.9, rot: [0, 0, 0] };
    const k = clamp((u - start) / RING_FLIGHT);
    const e = easeIn(k); // slow approach, then it slams home
    const after = Math.max(0, u - lock);
    const shudder = 0.03 * Math.exp(-after * 5.5) * Math.sin(after * 34);
    return { visible: true, scale: lerp(1.9, 1, e) + shudder, rot: ringSpin[i].map((r) => r * (1 - e)) };
  });
}
const lockPulse = (u) => RING_LOCKS.reduce((s, l) => s + 1.5 * decay(u, l, 0.3), 0);
const lockShake = (u) => RING_LOCKS.reduce((s, l, i) => s + (0.22 + 0.04 * i) * decay(u, l, 0.22), 0);

export const PLATES = {
  // Black. A heartbeat. Macro on the surface of something enormous.
  open: (u, d) => ({
    chapter: 'hero', p: 0, rings: HIDDEN,
    cam: { dolly: lerp(0.125, 0.105, easeInOut(u / d)), yaw: lerp(0.0, 0.18, u / d), lift: lerp(0.2, 0.0, u / d), sx: 0, sy: 0, shake: 0.012 },
    pulse: 0.25 * (0.5 + 0.5 * Math.sin(u * 2 * Math.PI * 1.16)) ** 4 + 0.05,
  }),
  // Pull back, fast, and see what it is: a star with no leash.
  reveal: (u, d) => ({
    chapter: 'hero', p: lerp(0.0, 0.2, u / d), rings: HIDDEN,
    cam: { dolly: lerp(0.105, 0.9, easeInOut(u / d)), yaw: lerp(0.18, 0.5, easeInOut(u / d)), lift: lerp(0.0, 1.6, easeInOut(u / d)), sx: lerp(0, 0, u / d), sy: 0, shake: lerp(0.05, 0.015, u / d) },
    pulse: flares(REVEAL_FLARES, u),
  }),
  // Wide, unbound, restless.
  tension: (u, d) => ({
    chapter: 'hero', p: lerp(0.2, 0.32, u / d), rings: HIDDEN,
    cam: { dolly: lerp(0.9, 0.78, easeInOut(u / d)), yaw: lerp(0.5, 0.78, u / d), lift: lerp(1.6, 1.1, u / d), sx: 0, sy: 0, shake: lerp(0.015, 0.06, easeIn(u / d)) },
    pulse: flares(TENSION_FLARES, u),
  }),
  // The harness closes: four rings, four hard limits, one by one.
  harness: (u, d) => ({
    chapter: 'hero', p: lerp(0.32, 0.5, u / d), rings: ringsAt(u),
    cam: {
      dolly: lerp(0.78, 0.58, easeInOut(u / d)) - 0.012 * RING_LOCKS.reduce((s, l) => s + decay(u, l, 0.4), 0),
      yaw: lerp(0.78, 0.4, easeInOut(u / d)), lift: lerp(1.1, 0.2, easeInOut(u / d)), sx: 0, sy: 0, shake: 0.015 + lockShake(u),
    },
    pulse: lockPulse(u) + 0.12,
  }),
  // Calm. Held in orbit.
  finale: (u, d) => ({
    chapter: 'hero', p: lerp(0.5, 1.0, u / d), rings: ringsAt(1e9),
    cam: { dolly: lerp(0.62, 1.2, easeInOut(u / d)), yaw: lerp(0.4, 0.1, u / d), lift: lerp(0.4, 0.0, u / d), sx: lerp(0.08, 0.2, easeInOut(u / d)), sy: 0, shake: 0 },
    pulse: 0.1 + 0.9 * decay(u, 0.2, 0.5),
  }),
};
