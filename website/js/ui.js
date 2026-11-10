// Page behaviour that does not need WebGL: navigation, reveal, copy buttons,
// tabs, and the two interactive illustrations (approvals, limits). The scene
// listens to what these write into `sim` and to the events they emit.

import { S, sim, on, emit, clamp } from './core.js';

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

export function initUI(chapters) {
  const live = el('div');
  live.className = 'sr-only';
  live.setAttribute('role', 'status');
  live.style.cssText = 'position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap';
  document.body.appendChild(live);
  const say = (t) => { live.textContent = ''; setTimeout(() => { live.textContent = t; }, 30); };

  const navApi = initNav();
  initReveal();
  initCopy(say);
  initTabs();
  const custody = initCustody();
  const limits = initLimits();
  const ultra = initUltra();
  initProviders();
  initDevice();

  let last = performance.now();
  function tick(now) {
    const dt = Math.min((now - last) / 1000, 0.1);
    last = now;
    chapters.update();
    navApi.update(chapters);
    custody.update(dt);
    limits.update(dt);
    ultra.update();
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
}

/* ── nav ─────────────────────────────────────────────────────────────── */
function initNav() {
  const nav = $('#nav'), toggle = $('#nav-toggle'), links = $('#nav-links');
  const onScroll = () => nav.classList.toggle('is-scrolled', scrollY > 24);
  onScroll();
  addEventListener('scroll', onScroll, { passive: true });
  toggle.addEventListener('click', () => {
    const open = toggle.getAttribute('aria-expanded') !== 'true';
    toggle.setAttribute('aria-expanded', String(open));
    links.classList.toggle('is-open', open);
  });
  links.addEventListener('click', (e) => {
    if (e.target.closest('a')) { toggle.setAttribute('aria-expanded', 'false'); links.classList.remove('is-open'); }
  });
  addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && links.classList.contains('is-open')) { toggle.click(); toggle.focus(); }
  });
  const anchors = $$('a', links);
  let cur = '';
  return {
    update(chapters) {
      let best = null, bw = 0.35;
      for (const it of chapters.items) if (it.w > bw && it.id !== 'hero') { best = it.id; bw = it.w; }
      if (best === cur) return;
      cur = best;
      anchors.forEach((a) => (a.getAttribute('href') === `#${cur}` ? a.setAttribute('aria-current', 'true') : a.removeAttribute('aria-current')));
    },
  };
}

/* ── reveal on scroll ────────────────────────────────────────────────── */
function initReveal() {
  const items = $$('.reveal');
  if (!('IntersectionObserver' in window) || S.reduced) {
    items.forEach((e) => e.classList.add('in'));
    return;
  }
  const io = new IntersectionObserver((entries) => {
    for (const en of entries) if (en.isIntersecting) { en.target.classList.add('in'); io.unobserve(en.target); }
  }, { threshold: 0.12, rootMargin: '0px 0px -6% 0px' });
  items.forEach((e) => io.observe(e));
  // anything already on screen at load
  requestAnimationFrame(() => items.forEach((e) => { const r = e.getBoundingClientRect(); if (r.top < innerHeight * 0.95 && r.bottom > 0) e.classList.add('in'); }));
}

/* ── copy ────────────────────────────────────────────────────────────── */
function initCopy(say) {
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-copy]');
    if (!b) return;
    const text = b.dataset.copy;
    let ok = false;
    try { await navigator.clipboard.writeText(text); ok = true; } catch {
      const ta = el('textarea'); ta.value = text; ta.style.cssText = 'position:fixed;opacity:0'; document.body.appendChild(ta); ta.select();
      try { ok = document.execCommand('copy'); } catch { /* ignore */ }
      ta.remove();
    }
    const prev = b.textContent;
    b.textContent = ok ? 'Copied' : 'Press Ctrl+C';
    b.classList.toggle('is-done', ok);
    say(ok ? 'Copied to clipboard' : 'Copy failed');
    setTimeout(() => { b.textContent = prev; b.classList.remove('is-done'); }, 1800);
  });
}

/* ── tabs ────────────────────────────────────────────────────────────── */
function initTabs() {
  const list = $('.tabs__list');
  if (!list) return;
  const tabs = $$('[role="tab"]', list);
  const select = (t, focus) => {
    tabs.forEach((x) => {
      const on_ = x === t;
      x.setAttribute('aria-selected', String(on_));
      x.tabIndex = on_ ? 0 : -1;
      document.getElementById(x.getAttribute('aria-controls')).hidden = !on_;
    });
    if (focus) t.focus();
  };
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => select(t));
    t.addEventListener('keydown', (e) => {
      const k = e.key;
      if (k === 'ArrowRight' || k === 'ArrowLeft' || k === 'Home' || k === 'End') {
        e.preventDefault();
        const n = k === 'Home' ? 0 : k === 'End' ? tabs.length - 1 : (i + (k === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
        select(tabs[n], true);
      }
    });
  });
}

/* ── 01 custody: approvals ───────────────────────────────────────────── */
const PROPOSALS = [
  { kind: 'edit', title: 'src/cli.rs', note: 'add a --verbose flag', lines: [['ctx', '@@ fn main()'], ['del', '-    init_logging();'], ['add', '+    #[arg(long)] verbose: bool,'], ['add', '+    init_logging(args.verbose);']] },
  { kind: 'run', title: 'cargo test --workspace', note: 'prove it works', lines: [['ctx', 'argv: ["cargo", "test", "--workspace"]'], ['ctx', 'cwd:  <disposable copy>']] },
  { kind: 'edit', title: 'README.md', note: 'document the flag', lines: [['ctx', '@@ ## Usage'], ['add', '+  rex run --verbose   print each step']] },
  { kind: 'run', title: 'git diff --stat', note: 'summarise the change', lines: [['ctx', 'argv: ["git", "diff", "--stat"]'], ['ctx', 'cwd:  <disposable copy>']] },
];

function initCustody() {
  const root = $('#demo-custody');
  const propEl = $('#proposal'), log = $('#runlog'), chipAuto = $('#auto-chip'), chipCount = $('#auto-count'), confirm = $('#auto-confirm');
  const seg = $$('.seg button', root);
  let confirmedOnce = false, idx = 0, nextAt = 0, arrivedAt = 0, current = null, id = 0;

  const setMode = (m) => {
    sim.mode = m;
    seg.forEach((b) => b.setAttribute('aria-checked', String(b.dataset.mode === m)));
    chipAuto.hidden = m !== 'auto';
    chipCount.hidden = m !== 'auto' || sim.autoCount === 0;
    emit('mode', m);
  };
  seg.forEach((b) => b.addEventListener('click', () => {
    const m = b.dataset.mode;
    if (m === sim.mode) return;
    if (m === 'auto' && !confirmedOnce) {
      confirm.hidden = false;
      $('[data-confirm="keep"]', confirm).focus({ preventScroll: true });
      return;
    }
    confirm.hidden = true;
    setMode(m);
  }));
  confirm.addEventListener('click', (e) => {
    const b = e.target.closest('[data-confirm]');
    if (!b) return;
    confirm.hidden = true;
    if (b.dataset.confirm === 'auto') { confirmedOnce = true; setMode('auto'); }
    seg.find((x) => x.dataset.mode === sim.mode).focus({ preventScroll: true });
  });

  const logLine = (cls, mark, text) => {
    const li = el('li', cls, text);
    li.dataset.mark = mark;
    log.prepend(li);
    while (log.children.length > 4) log.lastChild.remove();
  };

  const render = (p) => {
    propEl.textContent = '';
    if (!p) { propEl.append(el('p', 'proposal__wait', 'Waiting for the agent to propose something…')); return; }
    const head = el('div', 'proposal__head');
    head.append(el('b', '', `${p.kind === 'edit' ? 'write' : 'run'} · ${p.title}`), el('span', '', sim.mode === 'auto' ? 'auto' : 'needs approval'));
    const diff = el('pre', 'diff');
    p.lines.forEach(([c, t]) => { const s = el('span', c, t + '\n'); diff.append(s); });
    propEl.append(head, diff);
    if (sim.mode === 'build') {
      const row = el('div', 'proposal__row');
      const ok = el('button', 'btn btn--small btn--primary', 'Approve');
      const no = el('button', 'btn btn--small btn--ghost', 'Reject');
      ok.type = no.type = 'button';
      ok.disabled = no.disabled = !p.arrived;
      ok.addEventListener('click', () => decide('approve'));
      no.addEventListener('click', () => decide('reject'));
      row.append(ok, no);
      propEl.append(row);
    }
  };

  function decide(d, auto = false) {
    if (!current) return;
    const p = current;
    current = null;
    sim.proposal = null;
    emit('decision', { id: p.id, decision: d, auto, kind: p.kind });
    if (auto) {
      sim.autoCount++;
      chipCount.hidden = false;
      $('b', chipCount).textContent = String(sim.autoCount);
      logLine('auto', '▸', `Ran without asking · ${p.kind === 'edit' ? 'wrote' : 'ran'} ${p.title}`);
    } else if (d === 'approve') logLine('ok', '✓', `Approved · ${p.kind === 'edit' ? 'wrote' : 'ran'} ${p.title}`);
    else logLine('no', '✕', `Rejected · ${p.title} was not applied`);
    render(null);
    nextAt = performance.now() + (auto ? 1100 : 1500);
  }

  on('arrived', (id_) => { if (current && current.id === id_) { current.arrived = true; arrivedAt = performance.now(); render(current); } });
  setMode('build');

  return {
    update() {
      const now = performance.now();
      const active = S.w.custody > 0.4 && !document.hidden;
      if (!active) return;
      if (!current && now > nextAt) {
        const p = { ...PROPOSALS[idx++ % PROPOSALS.length], id: ++id, arrived: !sim.sceneLive };
        current = p;
        sim.proposal = p;
        arrivedAt = p.arrived ? now : 0;
        render(p);
        emit('propose', p);
      }
      if (current && sim.mode === 'auto' && current.arrived && now - arrivedAt > 650) decide('approve', true);
      // switching to Auto with a proposal waiting: let it through
      if (current && sim.mode === 'build' && !propEl.querySelector('.proposal__row')) render(current);
    },
  };
}

/* ── 02 limits: a run that hits a ceiling ────────────────────────────── */
function initLimits() {
  const root = $('#demo-limits');
  const meters = Object.fromEntries($$('.meter', root).map((m) => [m.dataset.limit, m]));
  const startBtn = $('#run-start'), resetBtn = $('#run-reset'), status = $('#run-status');
  const MAX = { steps: 40, tools: 80, tokens: 200000, time: 600 };
  const RATE = { steps: 1 / 13, tools: 1 / 16, tokens: 1 / 8.5, time: 1 / 21 }; // tokens run out first
  const NAMES = { steps: 'step', tools: 'tool-call', tokens: 'token', time: 'time' };
  const fmtTime = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
  const R = sim.run;

  const paint = () => {
    for (const k of Object.keys(meters)) {
      const f = clamp(R.fill[k]);
      const m = meters[k];
      $('.meter__bar b', m).style.width = `${(f * 100).toFixed(1)}%`;
      const v = k === 'time' ? `${fmtTime(MAX.time * f)} / ${fmtTime(MAX.time)}` : `${Math.round(MAX[k] * f).toLocaleString('en-US')} / ${MAX[k].toLocaleString('en-US')}`;
      $('.meter__v', m).textContent = v;
      m.classList.toggle('is-hit', R.halted && R.haltedBy === k);
    }
  };
  const reset = () => {
    R.active = false; R.halted = false; R.haltedBy = null; R.t = 0; R.touched = false;
    for (const k of Object.keys(R.fill)) R.fill[k] = 0;
    status.textContent = 'Idle. Four rings, four limits.';
    status.classList.remove('is-halt');
    startBtn.textContent = 'Start a run';
    paint();
    emit('run', R);
  };
  startBtn.addEventListener('click', () => {
    if (R.halted) reset();
    R.active = true; R.touched = true;
    status.textContent = 'Running…';
    status.classList.remove('is-halt');
    startBtn.textContent = 'Running';
    emit('run', R);
  });
  resetBtn.addEventListener('click', reset);

  return {
    update(dt) {
      if (!R.active) return;
      R.t += dt;
      for (const k of Object.keys(R.fill)) R.fill[k] = clamp(R.t * RATE[k] * (1 + 0.08 * Math.sin(R.t * 3 + k.length)));
      const hit = Object.keys(R.fill).find((k) => R.fill[k] >= 1);
      if (hit) {
        R.fill[hit] = 1;
        R.active = false; R.halted = true; R.haltedBy = hit;
        status.textContent = `Stopped: the ${NAMES[hit]} limit was reached. The run ends and says why (exit code 3).`;
        status.classList.add('is-halt');
        startBtn.textContent = 'Run again';
        emit('run', R);
      }
      paint();
    },
  };
}

/* ── 04 ultra: which step is lit ─────────────────────────────────────── */
function initUltra() {
  const items = $$('#ultra-steps li');
  const cuts = [0, 0.2, 0.4, 0.62, 0.8];
  let cur = -1;
  return {
    update() {
      const p = S.p.ultra ?? 0;
      let a = 0;
      for (let i = 0; i < cuts.length; i++) if (p >= cuts[i]) a = i;
      sim.ultraStep = a;
      if (a === cur) return;
      cur = a;
      items.forEach((li, i) => { li.classList.toggle('is-active', i === a); li.classList.toggle('is-done', i < a); });
    },
  };
}

/* ── 05 models ───────────────────────────────────────────────────────── */
function initProviders() {
  const list = $('#providers');
  if (!list) return;
  const set = (id) => { sim.provider = id; $$('button', list).forEach((b) => b.classList.toggle('is-hot', b.dataset.provider === id)); emit('provider', id); };
  list.addEventListener('pointerover', (e) => { const b = e.target.closest('button'); if (b) set(b.dataset.provider); });
  list.addEventListener('pointerleave', () => set(null));
  list.addEventListener('focusin', (e) => { const b = e.target.closest('button'); if (b) set(b.dataset.provider); });
  list.addEventListener('focusout', () => set(null));
}

/* ── 06 device tilt ──────────────────────────────────────────────────── */
function initDevice() {
  const dev = $('#device');
  if (!dev || S.reduced || !matchMedia('(hover: hover)').matches) return;
  const frame = $('.device__frame', dev);
  dev.addEventListener('pointermove', (e) => {
    const r = dev.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width, y = (e.clientY - r.top) / r.height;
    frame.style.setProperty('--ry', `${((x - 0.5) * 12).toFixed(2)}deg`);
    frame.style.setProperty('--rx', `${((0.5 - y) * 8).toFixed(2)}deg`);
    frame.style.setProperty('--gx', `${(x * 100).toFixed(0)}%`);
    frame.style.setProperty('--gy', `${(y * 100).toFixed(0)}%`);
  });
  dev.addEventListener('pointerleave', () => { frame.style.removeProperty('--ry'); frame.style.removeProperty('--rx'); });
}
