// Chapter 02: crisp HTML labels pinned to each ring of the harness, that follow
// the part of the ring facing a preferred side of the screen and update live
// with the demo (percentage used, and which limit stopped the run).

import { Vector3 } from 'three';
import { S, TAU } from './state.js';
import { sim } from '../core.js';
import { LIMITS } from './rings.js';

const PREFER = [[-1, 0.15], [-0.55, 0.95], [0.9, 0.7], [0.75, -0.9]]; // screen directions, y up

export function createLimitLabels({ labels, rings }) {
  const anchors = LIMITS.map(() => new Vector3());
  const tags = LIMITS.map((L, i) => labels.add({ text: L.label, sub: L.flag, anchor: (o) => o.copy(anchors[i]), chapter: 'limits', tiny: true }));
  const v = new Vector3();
  const A = 40;
  const cand = Array.from({ length: A }, (_, k) => new Vector3(Math.cos((k / A) * TAU), 0, Math.sin((k / A) * TAU)));
  const cur = LIMITS.map(() => -1);
  const proj = LIMITS.map(() => new Vector3());
  let camera = null;

  return {
    bind(cam) { camera = cam; },
    update() {
      const w = S.w.limits || 0;
      if (w < 0.02 || !camera) return;
      const R = sim.run;
      const W = innerWidth, H = innerHeight;
      rings.rings.forEach((r, i) => {
        r.group.updateWorldMatrix(true, false);
        let best = -1, bs = -1e9;
        for (let k = 0; k < A; k++) {
          v.copy(cand[k]).multiplyScalar(r.L.R + 0.07);
          r.group.localToWorld(v);
          v.project(camera);
          if (v.z > 1) continue;
          let s = v.x * PREFER[i][0] + v.y * PREFER[i][1];
          // keep clear of labels already placed for the inner rings
          for (let j = 0; j < i; j++) {
            const dx = ((v.x - proj[j].x) * W) / 2, dy = ((v.y - proj[j].y) * H) / 2;
            const d = Math.hypot(dx, dy);
            if (d < 130) s -= ((130 - d) / 130) * 3;
          }
          // hysteresis so the label does not hop between near-equal points
          if (s > bs + (k === cur[i] ? 0.06 : 0)) { bs = s; best = k; }
        }
        if (best >= 0) { cur[i] = best; v.copy(cand[best]).multiplyScalar(r.L.R + 0.07); r.group.localToWorld(v); anchors[i].copy(v); proj[i].copy(v).project(camera); }
        const L = r.L;
        const hit = R.halted && R.haltedBy === L.id;
        const sub = hit ? 'limit reached' : R.touched ? `${Math.round(Math.min(1, R.fill[L.id]) * 100)}% used` : L.flag;
        const el = tags[i].el;
        if (el.lastChild && el.lastChild.textContent !== sub) el.lastChild.textContent = sub;
        tags[i].state(hit ? 'is-warn' : '');
      });
    },
  };
}
