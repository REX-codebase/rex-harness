// "Page vitals": the page measures itself. A landing page that wants to be a
// benchmark should show its numbers, so this panel reads the real browser
// performance entries (no estimates): transfer size, request count, third-party
// requests, LCP and CLS, plus the live renderer's tier, frame rate and draw calls.

import { sim } from './core.js';

export function initVitals(params) {
  const panel = document.getElementById('vitals');
  const list = document.getElementById('vitals-list');
  const btn = document.getElementById('vitals-toggle');
  if (!panel || !list || !btn) return;

  let lcp = null, cls = 0;
  try {
    new PerformanceObserver((l) => { const e = l.getEntries(); lcp = e[e.length - 1].startTime; }).observe({ type: 'largest-contentful-paint', buffered: true });
    new PerformanceObserver((l) => { for (const e of l.getEntries()) if (!e.hadRecentInput) cls += e.value; }).observe({ type: 'layout-shift', buffered: true });
  } catch { /* unsupported: those rows say n/a */ }

  const rows = {};
  const add = (k, label) => {
    const dt = document.createElement('dt'), dd = document.createElement('dd');
    dt.textContent = label;
    list.append(dt, dd);
    rows[k] = dd;
  };
  [['tier', 'Quality tier'], ['fps', 'Frame rate'], ['res', 'Render scale'], ['calls', 'Draw calls'], ['tris', 'Triangles'], ['gpu', 'Adapter'],
    ['kb', 'Transferred'], ['req', 'Requests'], ['third', 'Third-party requests'], ['lcp', 'LCP'], ['cls', 'CLS']].forEach(([k, l]) => add(k, l));

  const set = (k, v, cls_) => { const e = rows[k]; if (e.textContent !== v) e.textContent = v; e.className = cls_ || ''; };

  function refresh() {
    const res = performance.getEntriesByType('resource');
    const nav = performance.getEntriesByType('navigation')[0];
    const all = nav ? [nav, ...res] : res;
    const bytes = all.reduce((s, e) => s + (e.transferSize || e.encodedBodySize || 0), 0);
    const third = res.filter((e) => { try { return new URL(e.name).origin !== location.origin; } catch { return false; } }).length;
    const rx = window.__rex;
    set('tier', rx ? `${rx.tier.name}${rx.gov.pinned ? ' (pinned)' : ''}` : 'no WebGL', rx ? '' : 'warn');
    set('fps', rx ? `${rx.gov.fps.toFixed(0)} fps` : 'n/a', rx && rx.gov.fps >= 50 ? 'good' : rx ? 'warn' : '');
    set('res', rx ? `${(rx.gov.scale * 100).toFixed(0)}%` : 'n/a');
    if (rx) {
      const i = rx.renderer.info.render;
      set('calls', String(i.calls));
      set('tris', i.triangles >= 1e6 ? `${(i.triangles / 1e6).toFixed(2)} M` : `${(i.triangles / 1e3).toFixed(0)} k`);
      set('gpu', rx.adapter.renderer.replace(/^ANGLE \((.*)\)$/, '$1').slice(0, 34));
    }
    set('kb', `${(bytes / 1024).toFixed(0)} KB`);
    set('req', String(all.length));
    set('third', String(third), third === 0 ? 'good' : 'warn');
    set('lcp', lcp == null ? 'n/a' : `${(lcp / 1000).toFixed(2)} s`, lcp != null && lcp < 2500 ? 'good' : lcp != null ? 'warn' : '');
    set('cls', cls.toFixed(3), cls < 0.1 ? 'good' : 'warn');
  }

  let timer = 0;
  const show = (open) => {
    panel.hidden = !open;
    btn.setAttribute('aria-expanded', String(open));
    clearInterval(timer);
    if (open) { refresh(); timer = setInterval(refresh, 500); }
  };
  btn.addEventListener('click', () => show(panel.hidden));
  addEventListener('keydown', (e) => {
    if ((e.key === 'v' || e.key === 'V') && !e.metaKey && !e.ctrlKey && !e.altKey && !/input|textarea|select/i.test(document.activeElement.tagName)) show(panel.hidden);
  });
  if (params.hud != null) show(true);
}
