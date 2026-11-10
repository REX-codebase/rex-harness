// The stage owns the WebGL context, the render loop and the quality governor.
// Everything visual lives in the other scene modules and is driven from here.

import { WebGLRenderer, Scene, PerspectiveCamera, Vector2, Vector3, NoToneMapping } from 'three';
import { U, S, clamp, damp, smooth } from './state.js';
import { sim, on } from '../core.js';
import { adapterInfo, pickTier, Governor } from './quality.js';
import { Post } from './post.js';
import { createSky } from './sky.js';
import { createCore } from './core.js';
import { createEnvironment } from './env.js';
import { createRings, LIMITS } from './rings.js';
import { createFlow } from './flow.js';
import { createRig } from './rig.js';
import { createLabels } from './labels.js';
import { createCustody } from './custody.js';
import { createProof } from './proof.js';
import { createUltra } from './ultra.js';
import { createConstellation } from './constellation.js';
import { createLimitLabels } from './limits.js';

// three.js frees nothing on its own: removing a mesh leaves its geometry, material
// and textures on the GPU. The shared environment map (scene.environment, referenced
// by mat.envMap) is deliberately left alone; everything else a part owns is released.
function disposeTree(root) {
  root.traverse((o) => {
    if (o.geometry) o.geometry.dispose();
    if (o.isInstancedMesh && o.dispose) o.dispose();
    const mats = o.material ? (Array.isArray(o.material) ? o.material : [o.material]) : [];
    for (const m of mats) {
      for (const k in m.uniforms || {}) {
        const v = m.uniforms[k].value;
        if (v && v.isTexture) v.dispose();
      }
      m.dispose();
    }
  });
}

const PIXEL_BUDGET = 4.4e6; // drawing-buffer pixels per frame, at most

export async function start({ canvas, params, reduced, chapters }) {
  const renderer = new WebGLRenderer({
    canvas, antialias: false, alpha: false, depth: false, stencil: false, powerPreference: 'high-performance',
  });
  const gl = renderer.getContext();
  if (!gl || !renderer.capabilities.isWebGL2) throw new Error('WebGL2 is required');
  renderer.toneMapping = NoToneMapping;
  renderer.setClearColor(0x000000, 1);
  renderer.info.autoReset = false; // one frame is many passes; vitals should count them all

  const adapter = adapterInfo(gl);
  const picked = pickTier(adapter, params);
  const gov = new Governor(picked.tier, picked.pinned, () => resize());
  const snap = params.snap != null;

  const scene = new Scene();
  const camera = new PerspectiveCamera(38, 1, 0.1, 2000);
  camera.position.set(0, 4, 21);

  let tier = gov.tier;
  let post = null;
  let dirty = true;
  let lost = false;
  const size = new Vector2();
  const parts = {};
  const envMap = createEnvironment(renderer);
  scene.environment = envMap;
  scene.environmentIntensity = 0.9;
  canvas.addEventListener('webglcontextlost', (e) => {
    // the GPU reset (driver update, sleep, tab pressure): show the CSS sky, never a black screen
    e.preventDefault();
    lost = true;
    document.documentElement.classList.remove('is-live');
  });
  canvas.addEventListener('webglcontextrestored', () => location.reload());
  const labels = createLabels(document.getElementById('labels'), camera);
  const rig = createRig(camera, chapters);
  const ctx = { renderer, camera, scene, envMap, labels, get quality() { return tier; } };
  const st = { fill: {}, glow: {}, halted: false };
  let limitLabels = null;

  const build = () => {
    labels.clear();
    for (const k of Object.keys(parts)) {
      if (parts[k].dispose) parts[k].dispose();
      scene.remove(parts[k].object);
      disposeTree(parts[k].object);
    }
    parts.sky = createSky(ctx);
    parts.core = createCore(ctx);
    parts.rings = createRings({ quality: tier, envMap });
    parts.flow = createFlow(ctx);
    parts.custody = createCustody(ctx);
    parts.proof = createProof(ctx);
    parts.ultra = createUltra(ctx);
    parts.constellation = createConstellation(ctx);
    for (const k of Object.keys(parts)) scene.add(parts[k].object);
    limitLabels = createLimitLabels({ labels, rings: parts.rings });
    limitLabels.bind(camera);
  };
  if (document.fonts && document.fonts.load) {
    await Promise.race([
      Promise.all([document.fonts.load('600 30px "REX Display"'), document.fonts.load('500 20px "REX Mono"')]),
      new Promise((r) => setTimeout(r, 1500)),
    ]).catch(() => {});
  }
  build();

  function resize() {
    if (gov.tier !== tier) {
      tier = gov.tier;
      if (post) post.dispose();
      post = null;
      build();
    }
    // pixel budget: the cost of a frame follows pixels, not CSS size
    const cap = Math.sqrt(PIXEL_BUDGET / (innerWidth * innerHeight));
    document.documentElement.dataset.tier = tier.name;
    const pr = Math.max(0.5, Math.min(devicePixelRatio || 1, tier.dprMax, cap) * gov.scale);
    renderer.setPixelRatio(pr);
    renderer.setSize(innerWidth, innerHeight, false);
    U.uPR.value = pr;
    // 4x MSAA on a half-float target is ~130 MB at 4 Mpx, so it steps down as the frame grows
    const px = innerWidth * innerHeight * pr * pr;
    const msaa = Math.min(tier.msaa, px <= 2.7e6 ? 4 : px <= 4.2e6 ? 2 : 0);
    if (post && post.msaa !== msaa) { post.dispose(); post = null; }
    if (!post) post = new Post(renderer, { msaa, levels: tier.bloomLevels });
    const sz = renderer.getDrawingBufferSize(size);
    post.setSize(sz.x, sz.y);
    dirty = true;
  }
  // the text panel of whichever chapter is most present, so labels can keep off it
  const panels = {};
  let panelRect = null, panelAt = -1;
  function activePanelRect() {
    if (innerWidth < 860) return null;
    if (state.frames % 10 === 0 || panelAt < 0) {
      let best = null, bw = 0.5;
      for (const it of chapters.items) if ((S.w[it.id] || 0) > bw) { best = it.id; bw = S.w[it.id]; }
      const el = best && (panels[best] || (panels[best] = document.querySelector(`[data-chapter="${best}"] .panel`)));
      if (el) { const r = el.getBoundingClientRect(); panelRect = { l: r.left, t: r.top, r: r.right, b: r.bottom }; } else panelRect = null;
      panelAt = 1;
    }
    return panelRect;
  }
  addEventListener('resize', () => resize());
  addEventListener('pointermove', (e) => {
    S.pointer.x = (e.clientX / innerWidth) * 2 - 1;
    S.pointer.y = (e.clientY / innerHeight) * 2 - 1;
    dirty = true;
  }, { passive: true });
  ['mode', 'run', 'decision', 'propose', 'arrived', 'provider'].forEach((ev) => on(ev, () => { dirty = true; }));
  resize();

  const tmp = new Vector3();
  const t0 = params.t != null ? params.t : reduced ? 14 : 0;
  const state = { t: t0, frames: 0, flowT: t0 };
  let last = performance.now();
  let alive = snap ? 1 : 0;
  let lastY = -1;

  function frame(now) {
    requestAnimationFrame(frame);
    if (lost) return;
    const raw = now - last;
    const dt = Math.min(raw / 1000, 0.1);
    gov.sample(raw);
    last = now;

    // reduced motion: draw only when something actually changed
    // (a running demo, or a proposal waiting at the gate, is a change in itself)
    if (sim.run.active || sim.proposal) dirty = true;
    if (reduced && !dirty && lastY === S.y && state.frames > 3 && alive >= 1) return;
    dirty = false;
    lastY = S.y;

    if (params.t == null && !reduced) state.t += dt;
    U.uTime.value = state.t;
    alive = Math.min(1, alive + dt * 0.55);
    U.uAlive.value = alive * alive * (3 - 2 * alive);
    post.params.uFade.value = smooth(0, 0.35, alive);
    post.params.uFlash.value = snap || reduced ? 0 : 0.55 * Math.exp(-Math.pow((alive - 0.62) * 9, 2));

    // a hard limit freezes the flow and tints it amber
    const R = sim.run;
    U.uFreeze.value = damp(U.uFreeze.value, R.halted ? 1 : 0, 4, dt);
    if (!reduced) state.flowT += dt * (1 - U.uFreeze.value);
    U.uFlowT.value = state.flowT;

    // Ultra is champagne, everything else silver and violet
    const tierTarget = smooth(0.1, 0.62, S.w.ultra || 0);
    U.uTier.value = snap ? tierTarget : damp(U.uTier.value, tierTarget, 3, dt);
    U.uPulse.value = damp(U.uPulse.value, 0, 3, dt);

    rig.update(dt, state.t, snap);

    // lensing is anchored on the entity wherever the camera is
    const D = Math.max(camera.position.length(), 1);
    U.uThetaE.value = clamp(2.1 / D, 0.035, 0.3);
    tmp.copy(camera.position).multiplyScalar(-1).normalize();
    U.uLensDir.value.copy(tmp);

    LIMITS.forEach((L, i) => {
      st.fill[L.id] = R.touched ? R.fill[L.id] : 0.42 + 0.3 * Math.sin(state.t * 0.17 + i * 1.7);
      st.glow[L.id] = R.active ? 0.35 + 0.15 * Math.sin(state.t * 6 + i) : R.halted && R.haltedBy === L.id ? 1 : 0;
    });
    st.halted = R.halted;

    renderer.info.reset();
    for (const k of Object.keys(parts)) if (parts[k].update) parts[k].update(dt, state.t, st);
    if (limitLabels) limitLabels.update();
    labels.avoid = activePanelRect();
    labels.update();
    post.params.uTier.value = U.uTier.value;
    post.render(scene, camera, state.t);
    state.frames++;
  }
  requestAnimationFrame(frame);

  // hooks for the headless checker (tools/verify.mjs and external harnesses)
  window.__ready = true;
  window.__renderInfo = () => ({
    calls: renderer.info.render.calls, triangles: renderer.info.render.triangles,
    geometries: renderer.info.memory.geometries, textures: renderer.info.memory.textures, programs: renderer.info.programs ? renderer.info.programs.length : 0,
  });
  window.__cycle = async () => { build(); await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); };

  const api = {
    renderer, scene, camera, gov, state, adapter, chapters, rig, parts,
    get tier() { return tier; },
    post: () => post,
    // test helper: jump to a chapter at a given progress
    scrollToChapter(id, p = 0.5) {
      const it = chapters.items.find((c) => c.id === id);
      if (!it) return false;
      scrollTo({ top: it.a + p * Math.max(0, it.b - it.a - innerHeight), behavior: 'instant' }); // p is progress over the pinned span
      return true;
    },
  };
  window.__rex = api;
  return api;
}
