import { Vector3 } from 'three';

// One set of uniform objects shared by every material in the scene, so a single
// write per frame reaches the sky, the stars, the core, the rings and the flow.
export const U = {
  uTime: { value: 0 }, // seconds, drives twinkle and plasma
  uFlowT: { value: 0 }, // seconds of *orbital* time; stops when a run hits a limit
  uTier: { value: 0 }, // 0 silver and violet, 1 Ultra champagne
  uPR: { value: 1 }, // drawing-buffer pixels per CSS pixel
  uLensDir: { value: new Vector3(0, 0, -1) }, // camera -> entity, world space
  uThetaE: { value: 0.1 }, // Einstein radius, radians
  uPulse: { value: 0 }, // brief brightness surge of the core
  uFreeze: { value: 0 }, // 1 when a hard limit has stopped the run
  uAlive: { value: 0 }, // ignition 0 -> 1 on load
};

export { S, TAU, clamp, lerp, smooth, damp } from '../core.js';
