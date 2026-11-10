#!/usr/bin/env node
// End-to-end verification in a real browser. Serves website/ itself, then runs the
// page through the situations a landing page has to survive and asserts on them:
//
//   node website/tools/verify.mjs [--out DIR] [--only desktop,laptop,short,phone,nowebgl,reduced,demos] [--chapters hero,custody,...] [--prefix /rex-harness/]
//
// Needs Playwright (a devDependency of the repo root: `npm ci`). Software GL is
// forced when there is no GPU, so frames are slow but real; this is a correctness
// check, not a performance measurement (use ?hud on real hardware for that).

import { createServer } from 'node:http';
import { readFileSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { join, extname, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

// require(), not import: it also finds a global Playwright install and NODE_PATH.
const { chromium } = createRequire(import.meta.url)('playwright');

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(`--${k}`); return i >= 0 ? args[i + 1] : d; };
const out = resolve(opt('out', join(process.env.TMPDIR || '/tmp', 'rex-site-verify')));
// GitHub Pages serves a project site under /<repo>/, not at the root. Serving the same way
// catches any root-absolute URL, which a root-served test would never notice.
const prefix = opt('prefix', '/rex-harness/');
const only = (opt('only', 'desktop,laptop,short,phone,nowebgl,reduced,demos')).split(',');
mkdirSync(out, { recursive: true });

const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.woff2': 'font/woff2', '.txt': 'text/plain', '.xml': 'application/xml', '.webmanifest': 'application/manifest+json' };
const server = createServer((req, res) => {
  let p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (prefix !== '/') { if (!p.startsWith(prefix)) { res.writeHead(404); res.end('not found (outside the Pages prefix)'); return; } p = '/' + p.slice(prefix.length); }
  if (p.endsWith('/')) p += 'index.html';
  const f = join(site, p);
  if (!f.startsWith(site) || !existsSync(f) || statSync(f).isDirectory()) { res.writeHead(404); res.end('not found'); return; }
  res.writeHead(200, { 'content-type': MIME[extname(f)] || 'application/octet-stream' });
  res.end(readFileSync(f));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;
const base = origin + prefix.replace(/\/$/, '');

const SCENARIOS = {
  desktop: { viewport: { width: 1440, height: 900 }, query: 'q=high&snap', webgl: true, panels: true },
  laptop: { viewport: { width: 1366, height: 768 }, query: 'q=medium&snap', webgl: true, panels: true },
  demos: { viewport: { width: 1440, height: 900 }, query: 'q=low&snap', webgl: true, panels: false, demos: true },
  short: { viewport: { width: 1280, height: 720 }, query: 'q=medium&snap', webgl: true, panels: true },
  phone: { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true, query: 'q=low&snap', webgl: true, panels: false },
  nowebgl: { viewport: { width: 1280, height: 800 }, query: 'nowebgl&snap', webgl: false, panels: false },
  reduced: { viewport: { width: 1280, height: 800 }, query: 'q=low&snap', reducedMotion: 'reduce', webgl: true, panels: false, contextLoss: true },
};
const CHAPTERS = opt('chapters', 'hero,custody,limits,proof,ultra,models,release,install').split(',');

let failed = 0;
const report = (scn, ok, msg) => { if (!ok) failed++; console.log(`${ok ? '  ok ' : ' FAIL'}  ${scn.padEnd(8)} ${msg}`); };

const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
for (const name of only) {
  const S = SCENARIOS[name];
  if (!S) continue;
  const ctx = await browser.newContext({ viewport: S.viewport, deviceScaleFactor: S.deviceScaleFactor || 1, isMobile: S.isMobile, hasTouch: S.hasTouch, reducedMotion: S.reducedMotion || 'no-preference' });
  const page = await ctx.newPage();
  const errors = [], external = [], failedReq = [];
  page.on('console', (m) => { if (m.type() === 'error' || (m.type() === 'warning' && /THREE|WebGL/.test(m.text()))) errors.push(m.text().slice(0, 300)); });
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('requestfailed', (r) => failedReq.push(r.url()));
  page.on('request', (r) => { if (!r.url().startsWith(origin) && !r.url().startsWith('data:') && !r.url().startsWith('blob:')) external.push(r.url()); });

  await page.goto(`${base}/index.html?${S.query}`, { waitUntil: 'load' });
  if (S.webgl) {
    const ok = await page.waitForFunction(() => window.__rex && window.__rex.state.frames > 2, null, { timeout: 120000 }).then(() => true, () => false);
    report(name, ok, ok ? 'scene started and rendered' : 'scene never rendered');
  } else {
    const ok = await page.waitForFunction(() => document.documentElement.classList.contains('no-webgl'), null, { timeout: 20000 }).then(() => true, () => false);
    report(name, ok, ok ? 'fell back to the CSS sky' : 'no-webgl class never set');
  }

  if (S.demos) {
    // The two interactive illustrations, driven the way a visitor drives them.
    const go = (id) => page.evaluate((c) => window.__rex.scrollToChapter(c, 0.5), id).then(() => page.waitForTimeout(800));
    const T = { timeout: 150000 };
    const step = async (label, fn) => { try { await fn(); report(name, true, label); } catch (e) { report(name, false, `${label}: ${String(e.message).split('\n')[0].slice(0, 140)}`); } };
    await go('custody');
    await step('a proposal reaches the gate and Approve enables', () => page.waitForSelector('#proposal .proposal__row button:not([disabled])', T));
    await step('Approve logs "Approved"', async () => { await page.click('#proposal .proposal__row button:has-text("Approve")'); await page.waitForSelector('#runlog li.ok', { timeout: 5000 }); });
    await step('the next proposal can be rejected and is logged', async () => { await page.waitForSelector('#proposal .proposal__row button:not([disabled])', T); await page.click('#proposal .proposal__row button:has-text("Reject")'); await page.waitForSelector('#runlog li.no', { timeout: 5000 }); });
    await step('choosing Auto asks first, with "Keep asking me" as the default focus', async () => {
      await page.click('.seg [data-mode="auto"]');
      await page.waitForSelector('#auto-confirm:not([hidden])', { timeout: 3000 });
      const f = await page.evaluate(() => document.activeElement && document.activeElement.dataset.confirm);
      if (f !== 'keep') throw new Error(`default focus is "${f}"`);
    });
    await step('"Keep asking me" leaves Build on', async () => { await page.click('[data-confirm="keep"]'); const m = await page.getAttribute('.seg [data-mode="build"]', 'aria-checked'); if (m !== 'true') throw new Error('Build is not selected'); });
    await step('confirming Auto shows the amber chip', async () => { await page.click('.seg [data-mode="auto"]'); await page.click('[data-confirm="auto"]'); await page.waitForSelector('#auto-chip:not([hidden])', { timeout: 3000 }); });
    await step('in Auto, actions run without asking and are counted', async () => { await page.waitForSelector('#runlog li.auto', T); const n = await page.textContent('#auto-count b'); if (!(+n >= 1)) throw new Error(`count is ${n}`); });
    await go('limits');
    await step('a run stops at its first limit, says so, and marks that meter', async () => {
      await page.click('#run-start');
      await page.waitForSelector('#run-status.is-halt', T);
      const hit = await page.$$eval('.meter.is-hit', (m) => m.map((x) => x.dataset.limit));
      if (hit.length !== 1) throw new Error(`hit meters: ${hit}`);
      const txt = await page.textContent('#run-status');
      if (!/Stopped/.test(txt) || !/exit code 3/.test(txt)) throw new Error(txt);
    });
    await page.waitForTimeout(2500);
    await page.screenshot({ path: join(out, 'demos-limits-halted.png'), timeout: 150000 });
    await step('Reset returns to idle', async () => { await page.click('#run-reset'); await page.waitForFunction(() => document.getElementById('run-status').textContent.startsWith('Idle')); });
  }

  if (S.webgl && S.panels && name === 'desktop') {
    // Quality-tier switches rebuild the scene. Rebuilding must not leak GPU resources.
    const mem = () => page.evaluate(() => window.__renderInfo());
    const before = await (async () => { await page.evaluate(() => window.__cycle()); return mem(); })();
    for (let i = 0; i < 3; i++) await page.evaluate(() => window.__cycle());
    const after = await mem();
    const grew = ['geometries', 'textures', 'programs'].filter((k) => after[k] > before[k]);
    report(name, grew.length === 0, grew.length ? `rebuild leaks: ${grew.map((k) => `${k} ${before[k]} -> ${after[k]}`).join(', ')}` : `no GPU resource growth over 3 rebuilds (${after.textures} textures, ${after.geometries} geometries)`);
  }

  for (const id of CHAPTERS) {
    await page.evaluate((c) => {
      const el = document.getElementById(c === 'hero' ? 'top' : c);
      const r = el.getBoundingClientRect();
      scrollTo({ top: Math.max(0, r.top + scrollY + (r.height - innerHeight) * 0.5), behavior: 'instant' });
    }, id);
    await page.waitForTimeout(S.webgl ? 1500 : 400);
    if (S.panels) {
      const clipped = await page.evaluate((c) => {
        const el = document.getElementById(c === 'hero' ? 'top' : c);
        const p = el.querySelector('.panel');
        // a panel that scrolls internally (very short windows) is not hiding anything
        return p && getComputedStyle(p).overflowY !== 'auto' ? p.scrollHeight - p.clientHeight : 0;
      }, id);
      if (clipped > 2) report(name, false, `${id}: panel content is clipped by ${clipped}px at ${S.viewport.width}x${S.viewport.height}`);
    }
    await page.screenshot({ path: join(out, `${name}-${id}.png`), timeout: 150000 });
  }
  if (S.isMobile) {
    await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
    await page.click('#nav-toggle');
    const open = await page.evaluate(() => document.getElementById('nav-links').classList.contains('is-open') && document.getElementById('nav-toggle').getAttribute('aria-expanded') === 'true');
    await page.keyboard.press('Escape');
    const closed = await page.evaluate(() => !document.getElementById('nav-links').classList.contains('is-open'));
    report(name, open && closed, open && closed ? 'menu opens, and Escape closes it' : `menu open=${open} closed-after-escape=${closed}`);
  }
  if (S.contextLoss) {
    // A GPU reset must degrade to the CSS sky, never to a black screen, and recover on restore.
    // keep the extension object: getExtension() returns null once the context is lost
    await page.evaluate(() => { window.__lose = window.__rex.renderer.getContext().getExtension('WEBGL_lose_context'); window.__lose.loseContext(); });
    await page.waitForTimeout(500);
    const live = await page.evaluate(() => document.documentElement.classList.contains('is-live'));
    report(name, !live, !live ? 'on GPU context loss the page falls back to the CSS sky' : 'page still claims to be live after context loss');
    const reload = page.waitForEvent('load', { timeout: 30000 }).then(() => true, () => false);
    await page.evaluate(() => window.__lose.restoreContext());
    report(name, await reload, 'on context restore the page reloads the scene');
  }
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
  report(name, overflow <= 0, overflow <= 0 ? 'no horizontal overflow' : `horizontal overflow of ${overflow}px`);
  report(name, errors.length === 0, errors.length ? `console problems: ${errors.slice(0, 3).join(' | ')}` : 'no console errors or three.js warnings');
  report(name, external.length === 0 && failedReq.length === 0, external.length || failedReq.length ? `external: ${external.join(', ')} failed: ${failedReq.join(', ')}` : 'zero third-party and zero failed requests');
  await ctx.close();
}
await browser.close();
server.close();
console.log(`\nscreenshots in ${out}`);
if (failed) { console.error(`${failed} check(s) failed`); process.exit(1); }
console.log('all verification checks passed');
