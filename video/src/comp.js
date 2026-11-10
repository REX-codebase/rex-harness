// REX Harness launch film: the compositor.
// window.setFrame(n) draws frame n: the recorded 3D plate, then everything the film lays over it.
// Every element is a pure function of time. No CSS transitions, no timers: render the same
// frame twice and you get the same pixels.

import { FPS, SHOTS, shotAt, plateFrame, DURATION } from './script.mjs';
import { PLATES } from './plates.mjs';

const $ = (s, r = document) => r.querySelector(s);
const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, t) => a + (b - a) * t;
const smooth = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
const easeOut = (x) => 1 - Math.pow(1 - clamp(x), 3);
const easeInOut = (x) => { x = clamp(x); return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2; };
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;');
const pad = (n) => String(n).padStart(5, '0');

const get = async (url, fallback) => { try { const r = await fetch(url); return r.ok ? await r.json() : fallback; } catch { return fallback; } };
const cues = await get('/meta/cues.json', null);
const vo = await get('/meta/vo.json', []);
const tracks = {};
for (const s of SHOTS) if (s.plate.legacy) tracks[s.plate.dir] = await get(`/legacy/${s.plate.dir}/tracks.json`, null);

const grain = $('#grain'), fade = $('#fade'), flash = $('#flash');
const plA = $('#plA'), plB = $('#plB'), subs = $('#subs'), hud = $('#hud');
const bloom = $('#bloom'), streak = $('#streak'), streakW = $('#streakW');
const slam = $('#slam'), slamWord = $('#slamWord'), slamFlag = $('#slamFlag'), haltEl = $('#halt');
const card = { mark: $('#cardMark'), tag: $('#cardTag'), sub: $('#cardSub'), meta: $('#cardMeta'), url: $('#cardUrl') };

// film grain: a seeded noise tile shifted every frame. It also dithers the dark gradients so they do not band after encoding.
{
  const c = document.createElement('canvas'); c.width = c.height = 256;
  const g = c.getContext('2d'); const im = g.createImageData(256, 256);
  let s = 12345; const rnd = () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
  for (let i = 0; i < im.data.length; i += 4) { const v = 128 + (rnd() - 0.5) * 120; im.data[i] = im.data[i + 1] = im.data[i + 2] = v; im.data[i + 3] = 255; }
  g.putImageData(im, 0, 0);
  grain.style.backgroundImage = `url(${c.toDataURL()})`;
}

// ── plates ──────────────────────────────────────────────────────────────────
// legacy footage gets a slow push-in so it moves like the rest of the film
const KB = { limits: [1.0, 1.12, 27], gate: [1.0, 1.1, 62], proof: [1.02, 1.1, 52], ultra: [1.0, 1.1, 36], models: [1.0, 1.12, 52] };
// where a cut is a dissolve rather than a continuation of the same camera move
const DISSOLVE = { limits: 0.6, gate: 0.5, proof: 0.5, ultra: 0.6, models: 0.5, finale: 0.9 };
const loaded = new Map();
async function setPlate(pl, shot, t, withTags) {
  const legacy = !!shot.plate.legacy;
  const tt = Math.min(t, shot.t1 - 0.5 / FPS);
  const f = plateFrame(shot, tt);
  const src = `/${legacy ? 'legacy' : 'plates'}/${shot.plate.dir}/${pad(f)}.jpg`;
  const img = $('img', pl);
  pl.className = `pl ${legacy ? 'legacy' : 'new'}`;
  if (loaded.get(img) !== src) { img.src = src; loaded.set(img, src); try { await img.decode(); } catch { /* a missing plate shows the void */ } }
  // the opening is graded dark and brightens as the camera pulls back
  const GR = { open: [0.5, 0.82], reveal: [0.82, 1.0] }[shot.id];
  pl.style.filter = GR ? `brightness(${lerp(GR[0], GR[1], easeInOut((tt - shot.t0) / (shot.t1 - shot.t0))).toFixed(3)})` : 'none';
  const kb = KB[shot.id];
  if (kb) {
    const z = lerp(kb[0], kb[1], easeInOut((tt - shot.t0) / (shot.t1 - shot.t0)));
    pl.style.transformOrigin = `${kb[2]}% 50%`; pl.style.transform = `scale(${z.toFixed(4)})`;
  } else { pl.style.transform = ''; }
  const tg = $('.tagsL', pl);
  const tr = withTags && legacy && tracks[shot.plate.dir] && tracks[shot.plate.dir][f];
  const tags = tr ? tr.tags : [];
  while (tg.children.length < tags.length) { const e = document.createElement('div'); e.className = 'tag3d'; tg.append(e); }
  [...tg.children].forEach((e, i) => {
    const g = tags[i];
    if (!g) { e.style.visibility = 'hidden'; return; }
    e.className = `tag3d ${g.c}`;
    e.innerHTML = esc(g.t) + (g.s ? `<small>${esc(g.s)}</small>` : '');
    e.style.transform = `translate(${g.x}px, ${g.y}px) translate(-50%, -50%)`;
    e.style.opacity = g.o.toFixed(2); e.style.visibility = 'visible';
  });
}

// ── flares: the core's pulses, as light ─────────────────────────────────────
const dec = (t, at, tau) => (t >= at ? Math.exp(-(t - at) / tau) : 0);
function flareAmp(t, shot) {
  let a = 0;
  if (shot.id === 'reveal') for (const f of cues.reveal.flares) a += f.a * dec(t, f.at, 0.32);
  if (shot.id === 'tension') for (const f of cues.tension.flares) a += f.a * dec(t, f.at, 0.32);
  if (shot.id === 'harness') for (const l of cues.harness.locks) a += 1.6 * dec(t, l, 0.3);
  if (shot.id === 'finale') a += 0.9 * dec(t, shot.t0 + 0.2, 0.5);
  return a;
}

// ── HUD ─────────────────────────────────────────────────────────────────────
function typed(text, t, at, cps) {
  const n = Math.floor(clamp((t - at) * cps, 0, text.length));
  return { s: text.slice(0, n), done: n >= text.length, started: t >= at };
}
const RING_COL = ['#a99af7', '#6aa8ff', '#d9a6ff', '#ffd7a0'];

function drawHud(t, shot) {
  const o = (a, b, x = 0.5) => clamp(smooth(a, a + x, t) * (1 - smooth(b - x, b, t)));
  if (shot.id === 'gate') {
    const g = cues.gate, vis = o(g.arrive - 0.2, shot.t1 - 0.1, 0.4);
    const approved = t >= g.approve, landed = t >= g.land;
    const blink = 0.55 + 0.45 * Math.sin(t * 7);
    hud.innerHTML = `<div class="blk" style="opacity:${vis.toFixed(2)}">
      <div class="k">Proposal</div><div class="v">write src/cli.rs</div>
      ${approved ? '<div class="v ok">&#10003; approved</div>' : `<div class="v warn" style="opacity:${blink.toFixed(2)}">&#9679; waiting for you</div>`}
      <div class="k" style="opacity:${landed ? 1 : 0}">applied to the disposable copy</div></div>`;
  } else if (shot.id === 'proof') {
    const a = typed('$ rex verify receipt.json', t, cues.typing[0].at, cues.typing[0].cps);
    const b = typed('$ rex replay receipt.json', t, cues.typing[1].at, cues.typing[1].cps);
    const done = t >= cues.proof.done;
    const cmd = (x) => `<div class="v"><span class="p">$</span>${esc(x.s.slice(1))}${x.started && !x.done ? '<span class="caret"></span>' : ''}</div>`;
    hud.innerHTML = `<div class="blk" style="opacity:${o(cues.typing[0].at - 0.2, shot.t1 - 0.1, 0.3).toFixed(2)}">
      ${a.started ? cmd(a) : ''}${b.started ? cmd(b) : ''}
      <div class="v ok" style="opacity:${done ? 1 : 0}">&#10003; checked offline</div></div>`;
  } else if (shot.id === 'ultra') {
    const names = ['Contract', 'Candidates', 'Adversary', 'Verifier', 'Promotion'];
    const st = cues.ultra.steps;
    let cur = -1; st.forEach((x, i) => { if (t >= x - 0.05) cur = i; });
    hud.innerHTML = `<div class="blk" style="opacity:${o(shot.t0 + 0.6, shot.t1, 0.5).toFixed(2)}"><div class="steps">${names.map((n, i) => `<span class="${i === cur ? 'on' : i < cur ? 'done' : ''}">${n}</span>`).join('')}</div></div>`;
  } else if (shot.id === 'models') {
    hud.innerHTML = `<div class="blk" style="opacity:${o(shot.t0 + 1.0, shot.t1, 0.5).toFixed(2)}"><div class="k">Thirteen routes</div><div class="v">no server &middot; no telemetry</div></div>`;
  } else hud.innerHTML = '';
}

// ── per-frame ───────────────────────────────────────────────────────────────
window.setFrame = async (f) => {
  const t = f / FPS;
  const shot = shotAt(t), u = t - shot.t0, d = shot.t1 - shot.t0;

  await setPlate(plA, shot, t, true);
  const idx = SHOTS.indexOf(shot), prev = SHOTS[idx - 1], dur = DISSOLVE[shot.id];
  if (prev && dur && u < dur) { await setPlate(plB, prev, prev.t1, false); plB.style.opacity = (1 - easeInOut(u / dur)).toFixed(3); plB.style.display = 'block'; }
  else plB.style.display = 'none';

  // flares, only on the new footage where the entity sits at a known point
  const amp = shot.plate.legacy ? 0 : flareAmp(t, shot);
  const cam = shot.plate.legacy ? {} : (PLATES[shot.id](u, d).cam || {});
  const cx = 960 + (cam.sx || 0) * 1920, cy = 402 + (cam.sy || 0) * 804;
  for (const el of [bloom, streak, streakW]) { el.style.left = cx + 'px'; el.style.top = cy + 'px'; }
  bloom.style.opacity = clamp(amp * 0.45, 0, 0.75).toFixed(3);
  streak.style.opacity = clamp(amp * 0.9, 0, 1).toFixed(3);
  streakW.style.opacity = clamp(amp * 0.55, 0, 0.85).toFixed(3);
  streak.style.transform = streakW.style.transform = `scaleX(${(0.7 + clamp(amp, 0, 2) * 0.35).toFixed(3)})`;

  // light and impact: a bloom flash on every ring lock, when the run hits its limit, and at the promotion
  let fl = 0;
  for (const l of cues.harness.locks) fl += 0.5 * dec(t, l, 0.14);
  fl += 0.3 * dec(t, cues.limits.halt, 0.2);
  fl += 0.28 * dec(t, cues.ultra.steps[4], 0.3);
  flash.style.opacity = clamp(fl, 0, 0.8).toFixed(3);

  // the ring-lock slam words
  let sIdx = -1; cues.harness.locks.forEach((l, i) => { if (t >= l) sIdx = i; });
  if (sIdx >= 0 && shot.id === 'harness') {
    const l = cues.harness.locks[sIdx], k = t - l;
    slamWord.textContent = cues.harness.names[sIdx]; slamFlag.textContent = cues.harness.flags[sIdx];
    slam.style.setProperty('--c', RING_COL[sIdx]);
    slam.style.opacity = (easeOut(k / 0.12) * (1 - smooth(l + 0.8, l + 1.15, t))).toFixed(3);
    slam.style.transform = `scale(${(1 + 0.14 * (1 - easeOut(k / 0.35))).toFixed(4)})`;
  } else slam.style.opacity = 0;

  // the limit
  const hk = t - cues.limits.halt;
  haltEl.style.opacity = shot.id === 'limits' && hk >= 0 ? (easeOut(hk / 0.25) * (1 - smooth(shot.t1 - 0.9, shot.t1 - 0.3, t))).toFixed(3) : '0';

  drawHud(t, shot);

  // end card
  const fin = shot.id === 'finale', F = cues.finale;
  const rvEl = (el, at, dy = 24, blur = 0) => { const k = easeOut((t - at) / 1.0); el.style.opacity = (fin ? k : 0).toFixed(3); el.style.transform = `translateY(${((1 - k) * dy).toFixed(1)}px)`; el.style.filter = blur && k < 0.99 ? `blur(${((1 - k) * blur).toFixed(1)}px)` : 'none'; };
  rvEl(card.mark, F.title, 18, 10);
  [...card.tag.querySelectorAll('.w')].forEach((w, i) => { const k = easeOut((t - (F.tag + i * 0.16)) / 0.9); w.style.opacity = (fin ? k : 0).toFixed(3); w.style.transform = `translateY(${((1 - k) * 34).toFixed(1)}px)`; w.style.filter = k < 0.99 ? `blur(${((1 - k) * 12).toFixed(1)}px)` : 'none'; });
  rvEl(card.sub, F.tag + 1.9, 14);
  rvEl(card.meta, F.tag + 3.0, 10);
  rvEl(card.url, F.tag + 3.8, 10);
  card.mark.parentElement.style.display = fin ? 'block' : 'none';

  // subtitles, in the bar
  let line = null;
  for (const v of vo) if (t >= v.at - 0.05 && t <= v.at + v.dur + 0.3) line = v;
  if (line) { const a = smooth(line.at - 0.05, line.at + 0.2, t) * (1 - smooth(line.at + line.dur + 0.05, line.at + line.dur + 0.3, t)); subs.innerHTML = `<span style="opacity:${a.toFixed(3)}">${esc(line.text)}</span>`; } else subs.innerHTML = '';

  // black at both ends: the film opens out of darkness and closes into it
  fade.style.opacity = Math.max(1 - smooth(0.4, 2.4, t), smooth(DURATION - 1.4, DURATION - 0.1, t)).toFixed(3);
  { const h = Math.imul(f + 1, 2654435761) >>> 0; grain.style.transform = `translate(${(h % 40) - 20}px, ${((h >>> 8) % 40) - 20}px)`; }
  await document.fonts.ready;
  return { shot: shot.id, u: +u.toFixed(2) };
};
window.__ready = true;
