// Entry point. The page is fully usable before any of the 3D loads: the UI,
// the demos and the copy all work on their own. The scene is an enhancement
// that starts after first paint, and a failure to start it is not an error the
// visitor ever sees: they get the CSS sky instead.

import { S, sim, Chapters } from './core.js';
import { initUI } from './ui.js';
import { initVitals } from './vitals.js';

const params = Object.fromEntries(new URLSearchParams(location.search));
if (params.t != null) params.t = parseFloat(params.t);
const html = document.documentElement;
if (params.snap != null) html.classList.add('no-anim');
if (params.bare != null) html.classList.add('bare'); // canvas only: for posters and social cards
S.reduced = matchMedia('(prefers-reduced-motion: reduce)').matches && params.motion !== '1';

function canWebGL2() {
  try {
    const c = document.createElement('canvas');
    const gl = c.getContext('webgl2');
    if (!gl) return false;
    const lose = gl.getExtension('WEBGL_lose_context');
    if (lose) lose.loseContext();
    return true;
  } catch {
    return false;
  }
}

const chapters = new Chapters();
initUI(chapters);
initVitals(params);

let measureTimer = 0;
const remeasure = () => { clearTimeout(measureTimer); measureTimer = setTimeout(() => { chapters.measure(); chapters.update(); }, 80); };
addEventListener('resize', remeasure);
addEventListener('load', remeasure);
if (document.fonts && document.fonts.ready) document.fonts.ready.then(remeasure);
if ('ResizeObserver' in window) new ResizeObserver(remeasure).observe(document.body);
chapters.update();

async function boot() {
  if (params.nowebgl != null || !canWebGL2()) { html.classList.add('no-webgl'); return; }
  try {
    const { start } = await import('./scene/stage.js');
    sim.sceneLive = true;
    await start({ canvas: document.getElementById('stage'), params, reduced: S.reduced, chapters });
    html.classList.add('has-webgl', 'is-live');
  } catch (err) {
    console.error('Scene failed to start; falling back to the static sky.', err);
    sim.sceneLive = false;
    html.classList.remove('has-webgl', 'is-live');
    html.classList.add('no-webgl');
  }
}

// start the heavy part only after the first paint, so LCP is the headline text
const idle = window.requestIdleCallback || ((f) => setTimeout(f, 60));
if (document.readyState === 'complete') idle(boot);
else addEventListener('load', () => idle(boot), { once: true });
