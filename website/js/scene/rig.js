// Camera rig. Each chapter has a pose (a function of its own progress and time),
// the rig blends between the two chapters the reading line sits between, then
// damps toward the result. `shift` is an off-axis lens shift: it moves the 3D
// subject sideways (or up on a phone) so it always sits in the free part of the
// screen next to the text panel, without ever rotating the camera.

import { Vector3 } from 'three';
import { S, lerp, damp, clamp, TAU } from './state.js';

const D2R = Math.PI / 180;
// position on a sphere of radius r around target; az 0 looks down -z
const orbit = (t, r, az, el) => [
  t[0] + r * Math.cos(el) * Math.sin(az),
  t[1] + r * Math.sin(el),
  t[2] + r * Math.cos(el) * Math.cos(az),
];

// Pipeline for the Custody chapter: the entity, a gate, and the disposable copy,
// laid out along one direction so a single camera reads them left to right.
export const PIPE = { dir: [Math.cos(38 * D2R), 0, -Math.sin(38 * D2R)], gate: 7.1, copy: 12.6 };
const PM = [PIPE.dir[0] * 6.0, 0, PIPE.dir[2] * 6.0];
// Where the receipt ledger column stands.
export const LEDGER = { x: 8.6, z: -3.2 };

export const POSES = {
  hero: (p, t) => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 21.5, 0.02 + p * 0.35 + Math.sin(t * 0.05) * 0.03, 11 * D2R), fov: 38, shift: [0.2, 0] }),
  custody: (p) => ({ target: PM, pos: orbit(PM, 33, -0.1 + p * 0.2, 9 * D2R), fov: 34, shift: [0.245, 0] }),
  limits: (p) => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 20.5, 0.7 - p * 0.9, 15 * D2R), fov: 36, shift: [-0.22, 0] }),
  proof: (p) => ({ target: [LEDGER.x * 0.55, 1.0, LEDGER.z], pos: orbit([LEDGER.x * 0.55, 1.0, LEDGER.z], 31, 0.16 - p * 0.2, 7 * D2R), fov: 36, shift: [0.23, 0] }),
  ultra: (p) => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 28.5, p * TAU * 0.5 - 0.3, (19 - 6 * p) * D2R), fov: 40, shift: [-0.2, 0] }),
  models: (p) => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 35, -0.2 + p * 0.4, 8 * D2R), fov: 40, shift: [0.2, 0] }),
  release: () => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 66, 0.25, 7 * D2R), fov: 38, shift: [0.34, 0.27] }),
  install: () => ({ target: [0, 0, 0], pos: orbit([0, 0, 0], 58, 0.1, 10 * D2R), fov: 38, shift: [0.33, 0.2] }),
};

export function createRig(camera, chapters) {
  const cur = { pos: new Vector3(0, 4, 21), look: new Vector3(), fov: 38, sx: 0, sy: 0, init: false };
  const A = { pos: new Vector3(), look: new Vector3(), fov: 0, sx: 0, sy: 0 };
  const B = { pos: new Vector3(), look: new Vector3(), fov: 0, sx: 0, sy: 0 };
  const T = { pos: new Vector3(), look: new Vector3(), fov: 38, sx: 0, sy: 0 };
  const px = { x: 0, y: 0 };
  const right = new Vector3(), up = new Vector3(0, 1, 0), fwd = new Vector3();

  function evalPose(id, p, t, out, phone, aspect) {
    const f = POSES[id] || POSES.hero;
    const r = f(p, t);
    out.look.fromArray(r.target);
    out.pos.fromArray(r.pos);
    out.fov = r.fov;
    out.sx = r.shift[0];
    out.sy = r.shift[1];
    if (phone) {
      // portrait: pull back in proportion to how narrow the screen is, and lift the subject above the panel
      const k = clamp(1.15 * Math.pow(1.6 / aspect, 0.72), 1.2, 2.5);
      out.pos.sub(out.look).multiplyScalar(k).add(out.look);
      out.sx = 0;
      out.sy = id === 'release' || id === 'install' ? 0.1 : id === 'hero' ? 0.3 : 0.2;
    }
  }

  return {
    pose: T,
    update(dt, t, snap) {
      const w = innerWidth, h = innerHeight, aspect = w / h;
      const phone = w < 860;
      const span = chapters.span();
      const items = chapters.items;
      if (!span || !items.length) return;
      const ia = items[span.i], ib = items[span.j];
      evalPose(ia.id, ia.p ?? 0, t, A, phone, aspect);
      if (span.i === span.j) {
        T.pos.copy(A.pos); T.look.copy(A.look); T.fov = A.fov; T.sx = A.sx; T.sy = A.sy;
      } else {
        evalPose(ib.id, ib.p ?? 0, t, B, phone, aspect);
        const e = span.e;
        T.pos.lerpVectors(A.pos, B.pos, e);
        T.look.lerpVectors(A.look, B.look, e);
        T.fov = lerp(A.fov, B.fov, e);
        T.sx = lerp(A.sx, B.sx, e);
        T.sy = lerp(A.sy, B.sy, e);
      }
      const k = snap || S.reduced ? 1 : 1 - Math.exp(-4.2 * dt);
      if (!cur.init) { cur.pos.copy(T.pos); cur.look.copy(T.look); cur.fov = T.fov; cur.sx = T.sx; cur.sy = T.sy; cur.init = true; }
      cur.pos.lerp(T.pos, k);
      cur.look.lerp(T.look, k);
      cur.fov = lerp(cur.fov, T.fov, k);
      cur.sx = lerp(cur.sx, T.sx, k);
      cur.sy = lerp(cur.sy, T.sy, k);

      // pointer parallax: a gentle slide in the camera's own plane
      px.x = damp(px.x, S.reduced ? 0 : S.pointer.x, 3, dt);
      px.y = damp(px.y, S.reduced ? 0 : S.pointer.y, 3, dt);
      fwd.subVectors(cur.look, cur.pos).normalize();
      right.crossVectors(fwd, up).normalize();
      camera.position.copy(cur.pos).addScaledVector(right, px.x * 0.9).addScaledVector(up, -px.y * 0.55);
      camera.lookAt(cur.look);
      camera.fov = cur.fov;
      camera.aspect = aspect;
      camera.setViewOffset(w, h, -cur.sx * w, cur.sy * h, w, h);
      camera.updateProjectionMatrix();
      camera.updateMatrixWorld();
    },
  };
}
