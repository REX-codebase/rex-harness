#!/usr/bin/env node
// Renders the REX Harness website's real WebGL scene, frame by frame, at 1080p.
//
//   node tools/render-plates.mjs --out DIR [--shots open,reveal] [--frames 0:90] [--quality high]
//   node tools/render-plates.mjs --legacy --out DIR      (the 16:9, 30 fps footage the film re-cuts: limits, custody, proof, ultra, models)
//   node tools/render-plates.mjs --probe '{"chapter":"hero","cam":{"dolly":0.14}}' --out DIR   (one test frame)
//
// How it stays deterministic and fast on a machine with no GPU:
//   - requestAnimationFrame and performance.now are replaced by a virtual clock; one call to
//     __step(33.3ms) runs exactly one frame of the site's own render loop.
//   - Math.random is seeded, so the particle bursts repeat.
//   - the canvas is read back in the same task as the draw (toDataURL), which skips the
//     compositor and takes ~1 s per frame on software GL.
//   - the page is the shipped website, unmodified on disk. Two lines of ui.js are patched in
//     memory (see patchUi) so the DOM demos don't fire their own events over the choreography.
//
// Besides each JPEG, a tracks.json records what the DOM would have drawn on top of the canvas:
// the label pills pinned to 3D points, and the state of the real limits demo.

import { createRequire } from 'node:module';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { SHOTS, FPS, W, H, PH, shotFrames } from '../src/script.mjs';
import { PLATES } from '../src/plates.mjs';
import { LEGACY_PLATES } from '../src/plates_legacy.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SITE = path.resolve(HERE, '../../website');

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : '1'] : null)).filter(Boolean));
const OUT = path.resolve(args.out || path.join(HERE, '../out/plates'));
const QUALITY = args.quality || 'high';
const only = args.shots ? args.shots.split(',') : SHOTS.filter((s) => !s.plate.legacy).map((s) => s.id);
const [fromF, toF] = args.frames ? args.frames.split(':').map(Number) : [0, Infinity];
const SPEED = 1.8; // the limits demo runs this much faster than on the page, so a run fits in a shot

// ── a tiny static server for website/ ───────────────────────────────────────
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.webp': 'image/webp', '.jpg': 'image/jpeg', '.png': 'image/png', '.svg': 'image/svg+xml', '.json': 'application/json', '.webmanifest': 'application/manifest+json' };
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  let f = path.join(SITE, decodeURIComponent(url.pathname));
  if (!f.startsWith(SITE)) { res.writeHead(403).end(); return; }
  if (fs.existsSync(f) && fs.statSync(f).isDirectory()) f = path.join(f, 'index.html');
  if (!fs.existsSync(f)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { 'content-type': MIME[path.extname(f)] || 'application/octet-stream' });
  fs.createReadStream(f).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const BASE = `http://127.0.0.1:${server.address().port}`;

function patchUi(src) {
  let out = src;
  const must = (cond, what) => { if (!cond) throw new Error(`ui.js changed upstream; cannot patch: ${what}`); };
  // the custody demo proposes on its own clock; the film proposes on its own
  must(out.includes("emit('propose', p);"), 'propose');
  out = out.replace("emit('propose', p);", '/* film: proposals are scripted */');
  // run the limits demo faster so a whole run fits inside one shot
  const re = /const RATE = \{[^}]*\};/;
  must(re.test(out), 'RATE');
  out = out.replace(re, `const RATE = { steps: ${SPEED} / 13, tools: ${SPEED} / 16, tokens: ${SPEED} / 8.5, time: ${SPEED} / 21 };`);
  return out;
}

const INIT = () => {
  // virtual clock
  let vnow = 0; let cbs = [];
  window.requestAnimationFrame = (cb) => { cbs.push(cb); return cbs.length; };
  window.cancelAnimationFrame = () => {};
  performance.now = () => vnow;
  window.__step = (ms) => { vnow += ms; const l = cbs; cbs = []; for (const cb of l) cb(vnow); };
  // seeded random
  let s = 0x9e3779b9;
  Math.random = () => { s |= 0; s = (s + 0x6d2b79f5) | 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
};

const PAGE_FRAME = async ({ spec, T0, first, quality, fps }) => {
  const r = window.__rex;
  if (!window.__core) window.__core = await import('./js/core.js');
  const { emit, sim } = window.__core;
  if (!window.__U) window.__U = (await import('./js/scene/state.js')).U;
  const U = window.__U;
  if (first) {
    r.state.t = T0; r.state.flowT = T0;
    const rig = r.rig, orig = rig.update;
    rig.update = function (dt, t, snap) {
      orig.call(this, dt, t, snap);
      const c = window.__cam;
      if (!c) return;
      const cam = r.camera, look = rig.pose.look, w = innerWidth, h = innerHeight;
      const off = cam.position.clone().sub(look);
      if (c.yaw) { const co = Math.cos(c.yaw), si = Math.sin(c.yaw); const x = off.x * co + off.z * si, z = -off.x * si + off.z * co; off.x = x; off.z = z; }
      if (c.lift) off.y += c.lift;
      if (c.dolly != null) off.multiplyScalar(c.dolly);
      if (c.shake) { const tt = r.state.t; off.x += c.shake * (Math.sin(tt * 41.3) + 0.6 * Math.sin(tt * 97.1 + 1)); off.y += c.shake * (Math.sin(tt * 37.7 + 2) + 0.6 * Math.sin(tt * 83.9)); off.z += c.shake * 0.5 * Math.sin(tt * 53.3 + 4); }
      cam.position.copy(look).add(off);
      cam.lookAt(look);
      if (c.sx != null) cam.setViewOffset(w, h, -c.sx * w, (c.sy || 0) * h, w, h);
      if (c.fovMul) cam.fov *= c.fovMul;
      cam.updateProjectionMatrix();
      cam.updateMatrixWorld();
    };
  }
  window.__cam = spec.cam || null;
  if (spec.rings) spec.rings.forEach((st, i) => {
    const ring = r.parts.rings.rings[i];
    ring.group.visible = st.visible;
    ring.group.scale.setScalar(Math.max(0.001, st.scale));
    ring.group.rotation.set(ring.L.tilt[0] + st.rot[0], ring.L.tilt[1] + st.rot[1], ring.L.tilt[2] + st.rot[2]);
  });
  if (spec.pulse != null) U.uPulse.value = spec.pulse;
  const it = r.chapters.items.find((c) => c.id === spec.chapter);
  scrollTo({ top: it.a + spec.p * Math.max(0, it.b - it.a - innerHeight), behavior: 'instant' });
  for (const ev of spec.fire || []) {
    if (ev.type === 'propose') emit('propose', { id: ev.id });
    else if (ev.type === 'decision') emit('decision', { id: ev.id, decision: ev.decision });
    else if (ev.type === 'mode') { sim.mode = ev.mode; emit('mode', ev.mode); }
    else if (ev.type === 'click') document.querySelector(ev.sel).click();
  }
  window.__step(1000 / fps);
  const canvas = document.getElementById('stage');
  const img = canvas.toDataURL('image/jpeg', 0.95);
  const tags = [];
  for (const e of document.querySelectorAll('#labels .tag3d')) {
    const o = parseFloat(e.style.opacity || '0');
    const m = /translate3d\(([-\d.]+)px,\s*([-\d.]+)px/.exec(e.style.transform || '');
    if (!m || o < 0.02) continue;
    const small = e.querySelector('small');
    tags.push({ t: e.firstChild ? e.firstChild.nodeValue : '', s: small ? small.textContent : '', x: +m[1], y: +m[2], o, c: e.className.replace('tag3d', '').trim() });
  }
  const R = sim.run;
  return { img, tags, run: { fill: { ...R.fill }, halted: R.halted, by: R.haltedBy, active: R.active, touched: R.touched }, mode: sim.mode, tier: r.tier.name };
};

const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl'] });

// ── probe: render test frames from JSON specs, for lining up a shot before committing to it ──
if (args.probe) {
  const specs = [].concat(JSON.parse(args.probe));
  const ctx = await browser.newContext({ viewport: { width: W, height: PH }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  await page.addInitScript(INIT);
  await page.route('**/js/ui.js', async (route) => route.fulfill({ contentType: 'text/javascript', body: patchUi(fs.readFileSync(path.join(SITE, 'js/ui.js'), 'utf8')) }));
  await page.goto(`${BASE}/index.html?snap&q=${QUALITY}`, { waitUntil: 'load' });
  await page.waitForFunction(() => window.__ready === true, null, { timeout: 240000, polling: 250 });
  await page.addStyleTag({ content: 'body *{visibility:hidden!important} #stage{visibility:visible!important;opacity:1!important}' });
  fs.mkdirSync(OUT, { recursive: true });
  for (const [i, spec] of specs.entries()) {
    spec.chapter ||= 'hero'; spec.p ??= 0; spec.fire = [];
    if (spec.rings === 'hidden') spec.rings = [0, 1, 2, 3].map(() => ({ visible: false, scale: 3, rot: [0, 0, 0] }));
    if (spec.rings === 'home') spec.rings = [0, 1, 2, 3].map(() => ({ visible: true, scale: 1, rot: [0, 0, 0] }));
    let out;
    for (let k = 0; k < 2; k++) out = await page.evaluate(PAGE_FRAME, { spec, T0: 3, first: i === 0 && k === 0, quality: QUALITY, fps: FPS });
    fs.writeFileSync(path.join(OUT, `probe_${i}.jpg`), Buffer.from(out.img.split(',')[1], 'base64'));
    console.log(`probe_${i}.jpg`, JSON.stringify(spec.cam || {}));
  }
  await browser.close(); server.close(); process.exit(0);
}

// A job is one continuous take. The film's own shots render at 1920 x 804 and 24 fps; the legacy footage
// (re-cut in the limits, gate, proof, ultra and models shots) rendered at 1920 x 1080 and 30 fps.
const LEGACY_JOBS = [['custody', 11.8, 11.6], ['limits', 10.6, 23.4], ['proof', 10.6, 34.0], ['ultra', 8.4, 44.6], ['models', 4.6, 53.0]]
  .map(([dir, dur, T0]) => ({ id: dir, dir, dur, T0, n: Math.round(dur * 30), fps: 30, h: H, program: LEGACY_PLATES[dir] }));
const NEW_JOBS = SHOTS.filter((s) => !s.plate.legacy && only.includes(s.id))
  .map((s) => ({ id: s.id, dir: s.plate.dir, dur: s.t1 - s.t0, T0: s.t0, n: shotFrames(s), fps: FPS, h: PH, program: PLATES[s.id] }));
const JOBS = args.legacy ? LEGACY_JOBS.filter((j) => !args.shots || only.includes(j.id)) : NEW_JOBS;

for (const shot of JOBS) {
  fs.mkdirSync(path.join(OUT, shot.dir), { recursive: true });
  const n = shot.n, dur = shot.dur;
  const dir = path.join(OUT, shot.dir);
  if (fs.existsSync(path.join(dir, '.done')) && fromF === 0 && toF === Infinity) { console.log(`${shot.id}: already rendered`); continue; }

  const ctx = await browser.newContext({ viewport: { width: W, height: shot.h }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  page.on('console', (m) => { if (m.type() === 'error') console.log(`[${shot.id}] page error:`, m.text().slice(0, 240)); });
  await page.addInitScript(INIT);
  await page.route('**/js/ui.js', async (route) => route.fulfill({ contentType: 'text/javascript', body: patchUi(fs.readFileSync(path.join(SITE, 'js/ui.js'), 'utf8')) }));
  await page.goto(`${BASE}/index.html?snap&q=${QUALITY}`, { waitUntil: 'load' });
  await page.waitForFunction(() => window.__ready === true, null, { timeout: 240000, polling: 250 });
  await page.addStyleTag({ content: 'body *{visibility:hidden!important} #stage{visibility:visible!important;opacity:1!important}' });

  const tracks = [];
  const fired = new Set();
  const t0 = Date.now();
  for (let f = 0; f < n; f++) {
    const u = f / shot.fps;
    const spec = shot.program(u, dur);
    spec.fire = (spec.events || []).filter((e, i) => !fired.has(i) && u >= e.at - 1e-9).map((e) => { fired.add((spec.events || []).indexOf(e)); return e; });
    if (args.preview && f >= toF) break;
    if (f < fromF || f >= toF) { // frames outside the requested window are still simulated, so state matches a full render
      await page.evaluate(PAGE_FRAME, { spec, T0: shot.T0, first: f === 0, quality: QUALITY, fps: shot.fps });
      tracks.push(null);
      continue;
    }
    const out = await page.evaluate(PAGE_FRAME, { spec, T0: shot.T0, first: f === 0, quality: QUALITY, fps: shot.fps });
    fs.writeFileSync(path.join(dir, `${String(f).padStart(5, '0')}.jpg`), Buffer.from(out.img.split(',')[1], 'base64'));
    tracks.push({ tags: out.tags, run: out.run, mode: out.mode });
    if (f === 0 && out.tier !== QUALITY) throw new Error(`scene booted at tier ${out.tier}, wanted ${QUALITY}`);
    if (f % 30 === 0) console.log(`${shot.id}: ${f}/${n}  ${((Date.now() - t0) / 1000 / (f + 1)).toFixed(2)} s/frame`);
  }
  if (!args.preview) fs.writeFileSync(path.join(dir, 'tracks.json'), JSON.stringify(tracks));
  if (!args.preview && fromF === 0 && toF === Infinity) fs.writeFileSync(path.join(dir, '.done'), String(Date.now()));
  console.log(`${shot.id}: done, ${n} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  await ctx.close();
}
await browser.close();
server.close();
