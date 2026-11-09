// Verifies the depth-and-motion layer (src/motion-ui.css, src/components/Fx.tsx)
// in real Chromium. Unlike a screenshot, each check samples computed styles
// over time, so it proves the motion runs, settles, and is removed by Reduce.
//
//   npm run build && npx vite preview --port 4173 &
//   node verification/motion-ui/verify-motion-ui.mjs [baseUrl]
//
// Exits 1 if any check fails. Writes screenshots next to this file.
import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const base = process.argv[2] || 'http://127.0.0.1:4173/';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, pass, detail) => { results.push({ name, pass: !!pass, detail }); console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail !== undefined ? `  ${JSON.stringify(detail)}` : ''}`); };

const browser = await chromium.launch({ headless: true });
const ctx = async (storage = {}, extra = {}) => {
  const c = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, ...extra });
  await c.addInitScript((s) => { try { for (const [k, v] of Object.entries(s)) if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v); } catch { /* storage unavailable */ } }, { 'rex-operator-mode': 'human', ...storage });
  return c;
};
const shot = (p, name, opts = {}) => p.screenshot({ path: path.join(here, name), ...opts });

// 1. Sliding tab pill: moves, overshoots (spring), and settles on the target.
{
  const c = await ctx(); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(1500);
  const ready = await p.evaluate(() => { const n = document.querySelector('.view-tabs'); return { ind: n.hasAttribute('data-ind'), live: n.hasAttribute('data-ind-live') }; });
  check('tab pill placed and transitions armed', ready.ind && ready.live, ready);
  await p.evaluate(() => {
    window.__s = []; const n = document.querySelector('.view-tabs'); const t0 = performance.now();
    const f = () => { window.__s.push(+new DOMMatrix(getComputedStyle(n, '::before').transform).m41.toFixed(1)); if (performance.now() - t0 < 900) requestAnimationFrame(f); };
    window.__go = () => requestAnimationFrame(f);
  });
  await p.evaluate(() => window.__go());
  await p.getByRole('button', { name: 'Settings', exact: true }).click();
  await p.waitForTimeout(1000);
  const xs = await p.evaluate(() => window.__s);
  const first = xs[0]; const last = xs[xs.length - 1]; const peak = Math.max(...xs);
  const target = await p.evaluate(() => { const b = [...document.querySelectorAll('.view-tabs button')].find((x) => x.classList.contains('is-current')); return b.offsetLeft; });
  check('tab pill travels to the new tab', last > first + 100 && Math.abs(last - target) < 1.5, { first, last, target });
  check('tab pill springs (overshoots, then settles)', peak > last + 2 && new Set(xs).size > 20, { peak, last, distinct: new Set(xs).size });
  await c.close();
}

// 2. Hero headline staggers in and ends fully opaque.
{
  const c = await ctx(); const p = await c.newPage();
  await p.goto(base, { waitUntil: 'commit' }); await p.waitForSelector('.fx-word');
  await sleep(250);
  const mid = await p.evaluate(() => [...document.querySelectorAll('.fx-word, .fx-sheen')].map((e) => +(+getComputedStyle(e).opacity).toFixed(2)));
  await sleep(1500);
  const end = await p.evaluate(() => [...document.querySelectorAll('.fx-word, .fx-sheen')].map((e) => +(+getComputedStyle(e).opacity).toFixed(2)));
  check('headline words arrive in sequence', mid[0] > mid[mid.length - 1] && mid.some((v) => v < 0.99), mid);
  check('headline ends fully visible', end.every((v) => v === 1), end);
  await p.waitForTimeout(800); await shot(p, '02-hero.png');
  const name = await p.evaluate(() => document.querySelector('h1').textContent);
  check('headline keeps its accessible text', name === 'Tell REX what to do.', name);
  await c.close();
}

// 3. Reduce (in-app): nothing is left invisible waiting on a delay, ambient layers are off.
{
  const c = await ctx({ 'rex-harness-motion': 'reduce' }); const p = await c.newPage();
  await p.goto(base, { waitUntil: 'commit' }); await p.waitForSelector('.fx-word'); await sleep(120);
  const r = await p.evaluate(() => ({
    shellReduced: document.querySelector('.harness-shell').classList.contains('reduced-fx'),
    hidden: [...document.querySelectorAll('h1 span, .task-field, .composer-meta, .example-prompt, .top-shell > *')].filter((e) => +getComputedStyle(e).opacity < 0.99).length,
    halo: getComputedStyle(document.querySelector('.amb-halo')).display,
    cone: getComputedStyle(document.querySelector('.amb-cone')).animationName,
    delayed: [...document.querySelectorAll('*')].filter((e) => parseFloat(getComputedStyle(e).animationDelay) > 0).length,
  }));
  check('reduce: shell resolved to reduced-fx', r.shellReduced);
  check('reduce: nothing hidden 120ms after load', r.hidden === 0, r.hidden);
  check('reduce: ambient halo and breathing removed', r.halo === 'none' && r.cone === 'none', r);
  check('reduce: no element carries an animation delay', r.delayed === 0, r.delayed);
  await c.close();
}

// 3b. Reduce also applies to the first-run gate (it had no motion classes before).
{
  const c = await ctx({ 'rex-harness-motion': 'reduce', 'rex-operator-mode': null }); const p = await c.newPage();
  await p.goto(base, { waitUntil: 'commit' }); await p.waitForSelector('.operator-gate'); await sleep(120);
  const g = await p.evaluate(() => ({ reduced: document.querySelector('.harness-shell').classList.contains('reduced-fx'), hidden: [...document.querySelectorAll('.operator-gate > *')].filter((e) => +getComputedStyle(e).opacity < 0.99).length }));
  check('reduce: first-run gate honours the preference', g.reduced && g.hidden === 0, g);
  await c.close();
}

// 3c. OS-level reduce in System mode.
{
  const c = await ctx({}, { reducedMotion: 'reduce' }); const p = await c.newPage();
  await p.goto(base, { waitUntil: 'commit' }); await p.waitForSelector('.fx-word'); await sleep(120);
  const g = await p.evaluate(() => ({ reduced: document.querySelector('.harness-shell').classList.contains('reduced-fx'), hidden: [...document.querySelectorAll('h1 span')].filter((e) => +getComputedStyle(e).opacity < 0.99).length }));
  check('system: OS reduce calms the layer too', g.reduced && g.hidden === 0, g);
  await c.close();
}

// 4. Entrance animations must not freeze hover/active transforms (fill-mode).
{
  const c = await ctx(); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(2200);
  await p.locator('#task-input').fill('x'); await p.waitForTimeout(400);
  const btn = p.locator('.task-field .run-button'); const box = await btn.boundingBox();
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await p.mouse.down(); await p.waitForTimeout(450);
  const t = await btn.evaluate((e) => getComputedStyle(e).transform);
  check('run button :active transform survives its entrance animation', t.startsWith('matrix(0.97'), t);
  await p.mouse.up();
  await c.close();
}

// 5. Pointer light follows the cursor inside the field.
{
  const c = await ctx(); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(1800);
  const f = await p.locator('.task-field').boundingBox();
  const read = () => p.locator('.task-field').evaluate((e) => [e.style.getPropertyValue('--mx'), e.style.getPropertyValue('--my')]);
  await p.mouse.move(f.x + 200, f.y + 80, { steps: 4 }); await p.waitForTimeout(150); const a = await read();
  await p.mouse.move(f.x + 600, f.y + 120, { steps: 4 }); await p.waitForTimeout(150); const b = await read();
  check('pointer light tracks the cursor', a[0] === '200.0px' && b[0] === '600.0px' && a[1] !== b[1], { a, b });
  await c.close();
}

// 6. Plan mode: the pill takes the plan tint and slides to Plan.
{
  const c = await ctx(); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(1800);
  const before = await p.evaluate(() => new DOMMatrix(getComputedStyle(document.querySelector('.mode-seg'), '::before').transform).m41);
  await p.getByRole('radio', { name: 'Plan' }).click(); await p.waitForTimeout(900);
  const after = await p.evaluate(() => new DOMMatrix(getComputedStyle(document.querySelector('.mode-seg'), '::before').transform).m41);
  check('build/plan pill slides to Plan', after > before + 20, { before, after });
  await shot(p, '03-plan-mode.png', { clip: { x: 300, y: 290, width: 840, height: 300 } });
  await c.close();
}

// 7. A sample run: rail lights the active stage, evidence prints in, done is celebrated.
{
  const c = await ctx({ 'rex-tour-mode': '1' }); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(900);
  await p.locator('#task-input').fill('Summarize the open issues and draft a changelog');
  await p.locator('#task-input').press('Control+Enter');
  await p.waitForSelector('.rail-stage[data-st="active"]');
  const live = await p.evaluate(() => ({ shell: document.querySelector('.harness-shell').dataset.run, active: document.querySelectorAll('.rail-stage[data-st="active"]').length, link: [...document.querySelectorAll('.rail-link')].map((l) => l.dataset.st), ripple: getComputedStyle(document.querySelector('.rail-stage[data-st="active"] .rail-node'), '::after').animationName }));
  check('working: exactly one active stage, ripple running, backdrop reads "working"', live.active === 1 && live.shell === 'working' && live.ripple === 'node-ripple', live);
  check('working: no link filled ahead of the active stage', !live.link.includes('complete'), live.link);
  await p.waitForTimeout(500); await shot(p, '04-run-working.png');
  await p.waitForSelector('.rail-stage[data-st="complete"] >> nth=2', { timeout: 20000 });
  await p.waitForTimeout(1800);
  const done = await p.evaluate(() => ({ shell: document.querySelector('.harness-shell').dataset.run, complete: document.querySelectorAll('.rail-stage[data-st="complete"]').length, bloom: getComputedStyle(document.querySelector('.amb-bloom')).animationName }));
  check('done: all stages complete, backdrop reads "done", bloom played', done.complete === 3 && done.shell === 'done' && done.bloom === 'amb-bloom', done);
  await p.waitForTimeout(1500); await shot(p, '05-run-done.png', { fullPage: true });
  await c.close();
}

// 8. Gate renders and is keyboard-operable with the new layer present.
{
  const c = await ctx({ 'rex-operator-mode': null }); const p = await c.newPage();
  await p.goto(base); await p.waitForTimeout(1400); await shot(p, '01-gate.png');
  // The radio group owns its keys, so focus must be inside it (as after Tab).
  await p.locator('.operator-option.is-active').focus();
  await p.keyboard.press('ArrowDown'); await p.waitForTimeout(300);
  const picked = await p.evaluate(() => document.querySelector('.operator-option.is-active b')?.textContent);
  check('gate: arrow keys still move the selection (with focus in the group)', picked === 'An agent', picked);
  await c.close();
}

// 9. No page errors in the main flows (the sidecar probe failing is expected offline).
{
  const c = await ctx(); const p = await c.newPage(); const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  await p.goto(base); await p.waitForTimeout(1500);
  for (const t of ['Terminal', 'Editor', 'Git', 'Settings', 'Task']) { await p.getByRole('button', { name: t, exact: true }).click(); await p.waitForTimeout(250); }
  check('no uncaught page errors while switching every view', errs.length === 0, errs);
  await c.close();
}

await browser.close();
fs.writeFileSync(path.join(here, 'checks.json'), JSON.stringify(results, null, 2));
const failed = results.filter((r) => !r.pass);
console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
process.exit(failed.length ? 1 : 0);
