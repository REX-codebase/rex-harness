#!/usr/bin/env node
// Static checks for the website. Runs in CI before every deploy and locally:
//   node website/tools/check.mjs
//
// It enforces the page's own promises, so the page cannot quietly stop being
// what it says it is: self-contained (no third-party requests), accessible
// markup basics, and a byte budget.

import { readFileSync, existsSync, statSync, readdirSync } from 'node:fs';
import { join, dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];
const notes = [];
const fail = (m) => failures.push(m);
const read = (p) => readFileSync(join(site, p), 'utf8');

// ── budgets (compressed bytes, what a visitor downloads) ──────────────────
// Set from the first real measurement (js 191 KB, css 8, fonts 98, img 32) with ~5 % headroom,
// not derived from a principle: the point is that growth is a decision, not an accident.
const BUDGET = { 'js (app + three)': 200 * 1024, css: 14 * 1024, fonts: 120 * 1024, img: 120 * 1024 };

const walk = (d) => readdirSync(d).flatMap((f) => { const p = join(d, f); return statSync(p).isDirectory() ? (f === 'tools' || f === 'node_modules' ? [] : walk(p)) : [p]; });
const files = walk(site);
const gz = (p) => gzipSync(readFileSync(p)).length;
const sum = (pred) => files.filter(pred).reduce((s, p) => s + (/\.(woff2|webp|png)$/.test(p) ? statSync(p).size : gz(p)), 0);
const sizes = {
  'js (app + three)': sum((p) => p.endsWith('.js')),
  css: sum((p) => p.endsWith('.css')),
  fonts: sum((p) => p.endsWith('.woff2')),
  img: sum((p) => /\/img\/.*\.(webp|png)$/.test(p) && !p.endsWith('poster.webp') && !p.endsWith('og.jpg') && !p.endsWith('apple-touch-icon.png')), // the poster is only fetched when WebGL is missing
};
for (const [k, v] of Object.entries(sizes)) {
  notes.push(`${k.padEnd(18)} ${(v / 1024).toFixed(1).padStart(7)} KB  (budget ${(BUDGET[k] / 1024).toFixed(0)} KB)`);
  if (v > BUDGET[k]) fail(`${k} is ${(v / 1024).toFixed(1)} KB, over its ${(BUDGET[k] / 1024).toFixed(0)} KB budget`);
}

// ── index.html ────────────────────────────────────────────────────────────
const html = read('index.html');

// every local reference resolves
for (const m of html.matchAll(/(?:src|href)="([^"#]+)"/g)) {
  const u = m[1];
  if (/^(https?:|mailto:|data:)/.test(u)) continue;
  if (!existsSync(join(site, u))) fail(`index.html references a missing file: ${u}`);
}
// css url() targets exist, relative to the stylesheet
for (const f of files.filter((p) => p.endsWith('.css'))) {
  for (const m of readFileSync(f, 'utf8').matchAll(/url\(\s*['"]?([^'")]+)['"]?\s*\)/g)) {
    const u = m[1];
    if (/^(https?:|data:|#)/.test(u)) continue;
    if (!existsSync(resolve(dirname(f), u))) fail(`${relative(site, f)} references a missing file: ${u}`);
  }
}

// no third-party resource loads. Plain <a href> links out are fine.
for (const m of html.matchAll(/<(script|link|img|iframe|source|video|audio)\b[^>]*\b(?:src|href)="(https?:\/\/[^"]+)"/g)) {
  const tag = m[0];
  if (/rel="canonical"/.test(tag) || /rel="manifest"/.test(tag)) continue;
  fail(`third-party resource loaded by <${m[1]}>: ${m[2]}`);
}
for (const f of files.filter((p) => p.endsWith('.css') || p.endsWith('.js'))) {
  const src = readFileSync(f, 'utf8');
  if (f.includes('/vendor/')) continue;
  for (const m of src.matchAll(/(?:url\(|import\(|from\s+|fetch\()\s*['"]?(https?:\/\/[^'")\s]+)/g)) fail(`${relative(site, f)} loads ${m[1]}`);
}

// import map points at a real file
const im = html.match(/<script type="importmap">([\s\S]*?)<\/script>/);
if (!im) fail('missing import map');
else { const map = JSON.parse(im[1]); if (!existsSync(join(site, map.imports.three))) fail('import map "three" target is missing'); }

// images: alt, width, height
for (const m of html.matchAll(/<img\b[^>]*>/g)) {
  for (const a of ['alt=', 'width=', 'height=']) if (!m[0].includes(a)) fail(`<img> without ${a.replace('=', '')}: ${m[0].slice(0, 80)}`);
}
// one h1, headings in order
const heads = [...html.matchAll(/<h([1-6])\b/g)].map((m) => +m[1]);
if (heads.filter((h) => h === 1).length !== 1) fail('there must be exactly one <h1>');
heads.reduce((prev, h) => { if (h > prev + 1) fail(`heading level jumps from h${prev} to h${h}`); return h; }, 1);
// ids unique; every aria-controls / href="#x" target exists
const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
const dup = ids.filter((x, i) => ids.indexOf(x) !== i);
if (dup.length) fail(`duplicate ids: ${[...new Set(dup)].join(', ')}`);
for (const m of html.matchAll(/(?:href="#|aria-controls="|aria-labelledby=")([^"]+)"/g)) if (m[1] && !ids.includes(m[1])) fail(`dangling reference to #${m[1]}`);
// language, title, description, JSON-LD
if (!/<html[^>]*\blang="en"/.test(html)) fail('<html> needs lang');
const title = (html.match(/<title>([^<]+)<\/title>/) || [])[1] || '';
if (title.length < 10 || title.length > 70) fail(`title length ${title.length} (want 10-70)`);
const desc = (html.match(/<meta name="description" content="([^"]+)"/) || [])[1] || '';
if (desc.length < 80 || desc.length > 170) fail(`description length ${desc.length} (want 80-170; search results truncate near 160)`);
const ld = html.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/);
try { JSON.parse(ld[1]); } catch { fail('JSON-LD does not parse'); }
for (const f of ['img/og.jpg', 'img/apple-touch-icon.png', 'favicon.svg', 'site.webmanifest', 'robots.txt', 'sitemap.xml', '404.html', '.nojekyll']) if (!existsSync(join(site, f))) fail(`missing ${f}`);

// the version pill is stamped from package.json at deploy; make sure the marker survives
if (!html.includes('data-rex-stamp>')) fail('version pill marker data-rex-stamp is missing');

console.log(notes.join('\n'));
if (failures.length) {
  console.error(`\n${failures.length} problem(s):\n - ${failures.join('\n - ')}`);
  process.exit(1);
}
console.log('\nsite checks passed');
