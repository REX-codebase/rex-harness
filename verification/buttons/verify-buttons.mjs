// Verifies the button system (src/buttons.css) in real Chromium by sampling
// computed styles, so it proves the properties that make buttons read as
// buttons rather than relying on a screenshot:
//   - a disabled primary stays recognisably the primary (dimmed accent, not a grey slab)
//   - secondary buttons have a visible edge and full-contrast text
//   - hover lifts, active presses, and Reduce / touch remove the lift
//   - Fast and Ultra say on or off, and each tier changes every button's corner radius
//   - the top bar fits at several widths, and inactive tabs are readable
//   - keyboard focus draws a ring
//
//   npm run build && npx vite preview --port 4173 &
//   node verification/buttons/verify-buttons.mjs [baseUrl]
//
// Exits 1 if any check fails. Writes checks.json next to this file.
import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const base = process.argv[2] || 'http://127.0.0.1:4173/';
const results = [];
const check = (name, pass, detail) => { results.push({ name, pass: !!pass, detail }); console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail !== undefined ? `  ${JSON.stringify(detail)}` : ''}`); };

const browser = await chromium.launch({ headless: true });
const errors = [];
const open = async ({ width = 1280, height = 900, motion = null, touch = false } = {}) => {
  const ctx = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 1, ...(touch ? { hasTouch: true, isMobile: true } : {}) });
  await ctx.addInitScript((m) => { try { localStorage.setItem('rex-operator-mode', 'human'); if (m) localStorage.setItem('rex-harness-motion', m); } catch { /* storage unavailable */ } }, motion);
  const p = await ctx.newPage();
  p.on('pageerror', (e) => errors.push(String(e)));
  await p.goto(base);
  await p.waitForSelector('.rex-wordmark');
  await p.waitForTimeout(2600); // entrances settle
  return { ctx, p };
};
const css = (p, sel, props, pseudo = null) => p.evaluate(([s, ps, pe]) => { const e = document.querySelector(s); if (!e) return null; const c = getComputedStyle(e, pe); return Object.fromEntries(ps.map((k) => [k, c[k]])); }, [sel, props, pseudo]);
// Chromium serialises color-mix() results as `color(srgb 0.56 0.52 0.74)` (0-1 floats) and plain colours as `rgb(r, g, b)` (0-255).
const rgb = (str) => {
  const n = (str.match(/[\d.]+/g) || []).slice(0, 3).map(Number);
  return /^color\(srgb/.test(str.trim()) ? n.map((v) => Math.round(v * 255)) : n;
};
const rgbNums = (m) => (m.match(/-?[\d.]+(?:e-?\d+)?/g) || []).map(Number);
const chroma = (str) => { const [r, g, b] = rgb(str); return Math.max(r, g, b) - Math.min(r, g, b); };
const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };

// 1. Primary: lit when ready, dimmed toward its own colour (not grey) when disabled.
{
  const { ctx, p } = await open();
  const dis = await css(p, '.run-button', ['backgroundColor', 'borderTopWidth', 'color', 'cursor', 'boxShadow']);
  check('disabled Run is a dimmed primary, not a grey slab (tinted text, 1px edge, not-allowed)', chroma(dis.color) > 35 && dis.borderTopWidth === '1px' && dis.cursor === 'not-allowed' && dis.boxShadow === 'none', dis);
  await p.fill('.task-input', 'Rename every pexels import');
  await p.waitForTimeout(500);
  const on = await css(p, '.run-button', ['backgroundImage', 'color', 'cursor', 'minHeight']);
  check('enabled Run is lit: gradient face, ink text, pointer, at least 42px tall', /gradient/.test(on.backgroundImage) && on.cursor === 'pointer' && parseFloat(on.minHeight) >= 42, on);
  check('the disabled and enabled Run differ in face, so typing visibly arms it', dis.backgroundColor !== (await css(p, '.run-button', ['backgroundColor'])).backgroundColor);
  const setup = await css(p, '.setup-btn.primary', ['backgroundImage']);
  check('setup primary shares the primary face', setup && /gradient/.test(setup.backgroundImage), setup);
  await ctx.close();
}

// 2. Secondary: a visible edge, full-contrast text, and "Connect a model" is a real control.
{
  const { ctx, p } = await open();
  const opt = await css(p, '.options-button', ['borderTopWidth', 'borderTopColor', 'color', 'backgroundImage', 'width', 'height']);
  check('secondary (options) has an edge, a raised face and a 40px square target', opt.borderTopWidth === '1px' && /gradient/.test(opt.backgroundImage) && opt.width === '40px' && opt.height === '40px', opt);
  const trig = await css(p, '.model-trigger', ['borderTopWidth', 'height', 'cursor']);
  check('"Connect a model" is a bordered 44px control, not a bare label', trig.borderTopWidth === '1px' && parseFloat(trig.height) >= 44 && trig.cursor === 'pointer', trig);
  const amber = await p.evaluate(() => { const e = document.querySelector('.model-trigger'); return e.matches(':has(.model-mark:not(.is-live))') ? getComputedStyle(e).borderTopColor : 'no-match'; });
  check('with no model connected it asks for attention (amber edge)', amber !== 'no-match' && rgb(amber)[0] > rgb(amber)[2], amber);
  const tab = await p.evaluate(() => { const b = [...document.querySelectorAll('.view-tabs button:not(.is-current)')][0]; const track = getComputedStyle(document.querySelector('.view-tabs')).backgroundColor; return { text: getComputedStyle(b).color, track }; });
  const trackRgb = rgb(tab.track);
  check('inactive tabs meet 4.5:1 contrast against their track', contrast(rgb(tab.text), trackRgb) >= 4.5, { text: tab.text, track: tab.track, ratio: +contrast(rgb(tab.text), trackRgb).toFixed(2) });
  await ctx.close();
}

// 3. States: hover lifts, active presses; Reduce and touch remove the lift.
{
  const { ctx, p } = await open();
  await p.hover('.model-trigger'); await p.waitForTimeout(450);
  const hov = await css(p, '.model-trigger', ['transform']);
  check('hover lifts a secondary button 1px', hov.transform === 'matrix(1, 0, 0, 1, 0, -1)', hov);
  await p.mouse.down(); await p.waitForTimeout(450);
  const act = await css(p, '.model-trigger', ['transform']);
  check('active presses it in (scaled down, no lift)', /^matrix\(0\.9\d*, 0, 0, 0\.9\d*, 0, 0\)$/.test(act.transform), act);
  await p.mouse.up();
  await p.mouse.move(5, 5); await p.waitForTimeout(300);
  await p.hover('.options-button'); await p.waitForTimeout(450);
  const tilt = await css(p, '.options-button', ['transform']);
  const [ma, mb] = rgbNums(tilt.transform);
  check('the options button keeps the motion layer\'s tilt on hover (rotate -8deg)', Math.abs(Math.atan2(mb, ma) * 180 / Math.PI + 8) < 0.6, tilt);
  await ctx.close();
}
{
  const { ctx, p } = await open({ motion: 'reduce' });
  const seen = {};
  for (const sel of ['.model-trigger', '.options-button', '.fast-control']) {
    await p.mouse.move(5, 5); await p.waitForTimeout(150);
    await p.hover(sel); await p.waitForTimeout(300);
    seen[sel] = (await css(p, sel, ['transform'])).transform;
  }
  check('Reduce: hover does not move secondary buttons or switches', Object.values(seen).every((t) => t === 'none'), seen);
  await ctx.close();
}
{
  const { ctx, p } = await open({ touch: true });
  const noHover = await p.evaluate(() => matchMedia('(hover: none)').matches);
  await p.locator('.options-button').dispatchEvent('mouseover'); await p.waitForTimeout(300);
  const t = await css(p, '.options-button', ['transform']);
  check('touch: (hover: none) matches and no lift sticks', noHover && t.transform === 'none', { noHover, transform: t.transform });
  await ctx.close();
}

// 4. Fast and Ultra say on or off; each tier sets every button's corner radius.
{
  const { ctx, p } = await open();
  const lamp = async (sel) => (await css(p, sel, ['content', 'width', 'backgroundColor', 'position'], '::after'));
  const fastOff = await lamp('.fast-control');
  const ultraOff = await lamp('.ultra-control');
  check('both switches carry a lamp (corner, 5px, absolutely placed)', fastOff.content !== 'none' && fastOff.width === '5px' && fastOff.position === 'absolute' && ultraOff.width === '5px', { fastOff, ultraOff });
  const radiusStd = await css(p, '.run-button', ['borderTopLeftRadius']);
  await p.click('.fast-control'); await p.waitForTimeout(1800);
  const fastOn = await lamp('.fast-control');
  const radiusFast = await css(p, '.run-button', ['borderTopLeftRadius']);
  await p.click('.fast-control'); await p.waitForTimeout(1500);
  check('Fast lights its lamp', fastOn.backgroundColor !== fastOff.backgroundColor, { off: fastOff.backgroundColor, on: fastOn.backgroundColor });
  await p.click('.ultra-control'); await p.waitForTimeout(2800);
  const ultraOn = await lamp('.ultra-control');
  const radiusUltra = await css(p, '.run-button', ['borderTopLeftRadius']);
  check('Ultra lights its lamp', ultraOn.backgroundColor !== ultraOff.backgroundColor, { off: ultraOff.backgroundColor, on: ultraOn.backgroundColor });
  check('radius follows the tier: 10px standard, 7px Fast, 4px Ultra', radiusStd.borderTopLeftRadius === '10px' && radiusFast.borderTopLeftRadius === '7px' && radiusUltra.borderTopLeftRadius === '4px', { std: radiusStd.borderTopLeftRadius, fast: radiusFast.borderTopLeftRadius, ultra: radiusUltra.borderTopLeftRadius });
  const glyph = await p.evaluate(() => [...document.querySelectorAll('.ultra-glyph i')].map((i) => getComputedStyle(i).top));
  check('Ultra glyph is a star in every state (all three bars share one pivot, never a menu)', glyph.every((t) => t === glyph[0]), glyph);
  await ctx.close();
}

// 5. The top bar fits: nothing clipped or overlapping, with Ultra on and off.
for (const width of [1280, 820, 640, 390]) {
  const { ctx, p } = await open({ width, height: 800 });
  const fit = () => p.evaluate(() => {
    const bar = document.querySelector('.top-shell'); const frame = document.querySelector('.brand-frame').getBoundingClientRect();
    const tabs = document.querySelector('.view-tabs').getBoundingClientRect(); const bn = document.querySelector('.brand-name');
    const nameShown = getComputedStyle(bn).display !== 'none';
    const clipped = nameShown && bn.scrollWidth > bn.clientWidth + 1;
    // In the wrapped phone layout the tabs sit on their own row, so horizontal overlap with the brand only counts when they share a row.
    const sameRow = Math.abs(tabs.top - frame.top) < 40;
    return { clipped, overlap: sameRow && tabs.left < frame.right - 1, pageOverflow: document.documentElement.scrollWidth - innerWidth, barOverflow: bar.scrollWidth - bar.clientWidth };
  });
  const off = await fit();
  await p.click('.ultra-control'); await p.waitForTimeout(2800);
  const on = await fit();
  check(`top bar fits at ${width}px, Ultra off and on (no clip, no overlap, no page overflow)`, [off, on].every((m) => !m.clipped && !m.overlap && m.pageOverflow <= 0), { off, on });
  await ctx.close();
}

// 6. Keyboard focus draws a ring.
{
  const { ctx, p } = await open();
  await p.focus('.task-input');
  let ring = null;
  for (let i = 0; i < 8 && !ring; i++) {
    await p.keyboard.press('Tab');
    ring = await p.evaluate(() => { const a = document.activeElement; if (!a || !a.matches('button')) return null; const c = getComputedStyle(a); return { cls: a.className.toString().split(' ')[0], style: c.outlineStyle, width: c.outlineWidth, offset: c.outlineOffset }; });
  }
  check('a focused button shows a 2px solid ring offset from its edge', ring && ring.style === 'solid' && ring.width === '2px' && parseFloat(ring.offset) >= 2, ring);
  await ctx.close();
}

check('no uncaught page errors', errors.length === 0, errors);
await browser.close();
fs.writeFileSync(path.join(here, 'checks.json'), JSON.stringify({ passed: results.filter((r) => r.pass).length, total: results.length, results }, null, 2));
const failed = results.filter((r) => !r.pass);
console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
process.exit(failed.length ? 1 : 0);
