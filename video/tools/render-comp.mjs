#!/usr/bin/env node
// Composites the film: loads src/comp.html and screenshots it once per frame.
//
//   node tools/render-comp.mjs --plates DIR --out DIR [--frames 0:300] [--every 15] [--shots title,cta]
//
//   --frames a:b   render only frames a..b-1
//   --every n      render every nth frame (contact sheets and reviews)
//   --shots ids    render only frames inside those shots
//   --list a,b,c   render exactly these frames

import { createRequire } from 'node:module';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { SHOTS, FPS, W, H, TOTAL_FRAMES } from '../src/script.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..');

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : '1'] : null)).filter(Boolean));
const PLATES = path.resolve(args.plates || path.join(ROOT, 'out/plates'));
const OUT = path.resolve(args.out || path.join(ROOT, 'out/frames'));
const [from, to] = args.frames ? args.frames.split(':').map(Number) : [0, TOTAL_FRAMES];
const every = +(args.every || 1);
const onlyShots = args.shots ? args.shots.split(',') : null;
const fmt = args.png ? 'png' : 'jpeg';
const list = args.list ? args.list.split(',').map(Number) : null;

const MOUNTS = [
  ['/src/', path.join(ROOT, 'src')],
  ['/fonts/', path.resolve(ROOT, '../website/fonts')],
  ['/img/', path.resolve(ROOT, '../docs/images')],
  ['/plates/', PLATES],
];
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.jpg': 'image/jpeg', '.png': 'image/png', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  const m = MOUNTS.find(([pre]) => p.startsWith(pre));
  const f = m && path.join(m[1], p.slice(m[0].length));
  if (!m || !f.startsWith(m[1]) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) { res.writeHead(404).end(); return; }
  res.writeHead(200, { 'content-type': MIME[path.extname(f)] || 'application/octet-stream', 'cache-control': 'no-store' });
  fs.createReadStream(f).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const BASE = `http://127.0.0.1:${server.address().port}`;

fs.mkdirSync(OUT, { recursive: true });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
page.on('console', (m) => { if (['error', 'warning'].includes(m.type())) console.log('page', m.type(), m.text().slice(0, 240)); });
page.on('pageerror', (e) => console.log('page exception:', e.message));
await page.goto(`${BASE}/src/comp.html`, { waitUntil: 'load' });
await page.waitForFunction(() => window.__ready === true, null, { timeout: 60000, polling: 100 });

const inShots = (f) => !onlyShots || onlyShots.some((id) => { const s = SHOTS.find((x) => x.id === id); return f >= Math.round(s.t0 * FPS) && f < Math.round(s.t1 * FPS); });
const t0 = Date.now(); let n = 0;
const frames = list || Array.from({ length: Math.max(0, Math.ceil((Math.min(to, TOTAL_FRAMES) - from) / every)) }, (_, i) => from + i * every);
for (const f of frames) {
  if (!inShots(f)) continue;
  await page.evaluate((f) => window.setFrame(f), f);
  await page.screenshot({ path: path.join(OUT, `${String(f).padStart(5, '0')}.${fmt === 'png' ? 'png' : 'jpg'}`), type: fmt, ...(fmt === 'jpeg' ? { quality: 96 } : {}) });
  if (++n % 60 === 0) console.log(`comp: frame ${f}  ${((Date.now() - t0) / 1000 / n).toFixed(2)} s/frame`);
}
console.log(`comp: ${n} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
await browser.close();
server.close();
