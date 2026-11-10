// Shared between the page UI and the WebGL scene. No three.js import here, so
// the UI, the demos and the chapter logic keep working if WebGL never starts.

export const TAU = Math.PI * 2;
export const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
export const lerp = (a, b, t) => a + (b - a) * t;
export const smooth = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};
export const damp = (cur, target, rate, dt) => cur + (target - cur) * (1 - Math.exp(-rate * dt));

// Per-frame facts read by the scene.
export const S = {
  w: {}, // chapter weight 0..1 by id (how "present" a chapter is)
  p: {}, // progress 0..1 through each chapter
  y: 0, // scroll position of the reading line
  vh: 1,
  pointer: { x: 0, y: 0 },
  reduced: false,
};

// State the demos write and the scene reads (and a little the other way).
export const sim = {
  mode: 'build', // build | auto
  proposal: null, // { id, kind, title, lines } while a proposal waits at the gate
  decision: null, // 'approve' | 'reject' set by the UI, consumed by the scene
  autoCount: 0,
  run: { active: false, t: 0, halted: false, haltedBy: null, fill: { steps: 0, tools: 0, tokens: 0, time: 0 } },
  provider: null, // id of the model route being hovered
  tier: 'off',
};

const handlers = new Map();
export const on = (ev, fn) => {
  if (!handlers.has(ev)) handlers.set(ev, new Set());
  handlers.get(ev).add(fn);
  return () => handlers.get(ev).delete(fn);
};
export const emit = (ev, data) => {
  const set = handlers.get(ev);
  if (set) set.forEach((fn) => fn(data));
};

// Chapter engine. Reads <section data-chapter="id"> boxes once (and on resize),
// then maps a scroll position to a weight and a progress per chapter.
export class Chapters {
  constructor() {
    this.items = [];
    this.measure();
  }

  measure() {
    const vh = innerHeight;
    S.vh = vh;
    const sy = scrollY;
    this.items = [...document.querySelectorAll('[data-chapter]')].map((el) => {
      const r = el.getBoundingClientRect();
      const a = r.top + sy, b = r.bottom + sy;
      return { id: el.dataset.chapter, el, a, b, c: (a + b) / 2 };
    });
  }

  // y: scrollY. The "reading line" sits at the middle of the viewport.
  //
  // A chapter's copy is pinned (position: sticky) from the moment its top reaches
  // the top of the screen until its bottom reaches the bottom. So:
  //   - progress runs over that pinned span, which is when the reader is actually
  //     looking at the chapter, not over its whole box;
  //   - the 3D hands over from one chapter to the next across the boundary,
  //     while the old panel leaves and the new one arrives.
  update(y = scrollY) {
    const vh = S.vh;
    const ry = y + vh * 0.5;
    S.y = ry;
    for (const it of this.items) {
      const p0 = it.a + vh * 0.5, p1 = Math.max(p0 + 1, it.b - vh * 0.5);
      it.p = clamp((ry - p0) / (p1 - p0));
      const enter = smooth(it.a - vh * 0.5, it.a + vh * 0.4, ry);
      const leave = 1 - smooth(it.b - vh * 0.5, it.b + vh * 0.4, ry);
      it.w = enter * leave;
      S.p[it.id] = it.p;
      S.w[it.id] = it.w;
    }
    return ry;
  }

  // The chapter pair the camera is between and how far across the hand-over it is.
  span(ry = S.y) {
    const it = this.items;
    if (!it.length) return null;
    const vh = S.vh;
    let j = 0;
    for (let k = 0; k < it.length; k++) if (ry >= it[k].a - vh * 0.5) j = k;
    if (j === 0) return { i: 0, j: 0, e: 0 };
    const u = clamp((ry - (it[j].a - vh * 0.5)) / vh);
    return u >= 1 ? { i: j, j, e: 0 } : { i: j - 1, j, e: smooth(0, 1, u) };
  }
}
