// HTML labels pinned to 3D points. Text stays crisp and accessible-adjacent
// (the same information is always in the page copy); only a transform is
// written per frame, so there is no layout work.

import { Vector3 } from 'three';
import { S } from './state.js';

export function createLabels(container, camera) {
  const items = [];
  const v = new Vector3();

  return {
    avoid: null, // { l, t, r, b } of the text panel on screen, or null
    add({ text, sub, anchor, chapter, dx = 0, dy = -22, cls = '', avoidPanel = false, tiny = false }) {
      const el = document.createElement('div');
      el.className = `tag3d ${cls}${tiny ? ' no-sub-sm' : ''}`.trim();
      el.textContent = text;
      if (sub) { const s = document.createElement('small'); s.textContent = sub; el.append(s); }
      container.append(el);
      const it = { el, anchor, chapter, dx, dy, alpha: 1, shown: 0, base: cls, avoidPanel, w: 0 };
      items.push(it);
      return {
        el,
        set alpha(a) { it.alpha = a; },
        state(c) { el.className = `tag3d ${it.base} ${c || ''}${tiny ? ' no-sub-sm' : ''}`.trim(); },
        text(t) { el.firstChild.nodeValue = t; },
      };
    },
    clear() {
      for (const it of items) it.el.remove();
      items.length = 0;
    },
    update() {
      const W = innerWidth, H = innerHeight, av = this.avoid;
      for (const it of items) {
        const w = (it.chapter ? S.w[it.chapter] || 0 : 1) * it.alpha;
        if (w < 0.02) {
          if (it.shown) { it.el.style.opacity = '0'; it.shown = 0; }
          continue;
        }
        if (typeof it.anchor === 'function') it.anchor(v);
        else if (it.anchor.getWorldPosition) it.anchor.getWorldPosition(v);
        else v.copy(it.anchor);
        v.project(camera);
        const vis = v.z > -1 && v.z < 1 && Math.abs(v.x) < 1.08 && Math.abs(v.y) < 1.08;
        if (!vis) {
          if (it.shown) { it.el.style.opacity = '0'; it.shown = 0; }
          continue;
        }
        const x = (v.x * 0.5 + 0.5) * W + it.dx, y = (-v.y * 0.5 + 0.5) * H + it.dy;
        if (!it.w) it.w = it.el.offsetWidth || 90;
        const half = it.w / 2 + 10;
        const clipped = x - half < 0 || x + half > W || y < 76 || y > H - 12;
        const onPanel = it.avoidPanel && av && x + half > av.l && x - half < av.r && y + 16 > av.t && y - 16 < av.b;
        if (clipped || onPanel) {
          if (it.shown) { it.el.style.opacity = '0'; it.shown = 0; }
          continue;
        }
        it.el.style.transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0) translate(-50%, -50%)`;
        it.el.style.opacity = w.toFixed(2);
        it.shown = 1;
      }
    },
  };
}
