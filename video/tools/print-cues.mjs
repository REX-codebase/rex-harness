#!/usr/bin/env node
// Prints the sound cue sheet as JSON, derived from the same timeline the picture uses.
//   node tools/print-cues.mjs [--plates DIR] > out/cues.json
// If the limits plate has been rendered, the moment the real demo hit its token limit is read
// from its recorded tracks, so the "stop" lands on the frame where the rings freeze.
import fs from 'node:fs';
import path from 'node:path';
import { SHOTS, FPS, DURATION } from '../src/script.mjs';
import { CUSTODY_EVENTS, LIMITS_START } from '../src/plates.mjs';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1]] : null)).filter(Boolean));
const S = Object.fromEntries(SHOTS.map((s) => [s.id, s]));
const abs = (id, u) => +(S[id].t0 + u).toFixed(3);

const ev = Object.fromEntries(CUSTODY_EVENTS.map((e) => [`${e.type}${e.id ?? ''}`, e.at]));
let haltU = LIMITS_START + 4.7; // estimate: tokens at 1.8x speed
if (args.plates) {
  const f = path.join(args.plates, 'limits/tracks.json');
  if (fs.existsSync(f)) {
    const tr = JSON.parse(fs.readFileSync(f, 'utf8'));
    const i = tr.findIndex((x) => x && x.run && x.run.halted);
    if (i >= 0) haltU = i / FPS;
  }
}

// typed-text schedules: [shot, start u, characters, chars per second]. Keep in sync with src/comp.js.
const typing = [
  ['limits', 1.6, 56, 62], ['limits', 2.55, 31, 40], ['limits', 3.2, 52, 60],
  ['proof', 2.0, 49, 58], ['proof', 2.9, 22, 55], ['proof', 3.6, 24, 40], ['proof', 4.3, 48, 70], ['proof', 5.3, 24, 40], ['proof', 6.0, 48, 70],
].map(([id, u, n, cps]) => ({ at: abs(id, u), n, cps }));

const cues = {
  duration: DURATION,
  fps: FPS,
  shots: Object.fromEntries(SHOTS.map((s) => [s.id, { t0: s.t0, t1: s.t1 }])),
  hook: { q: [abs('hook', 3.3), abs('hook', 4.0), abs('hook', 4.7)], riserFrom: abs('hook', 2.9) },
  custody: {
    propose1: abs('custody', ev.propose1), arrive1: abs('custody', ev.propose1 + 2.3), approve: abs('custody', ev.decision1),
    land1: abs('custody', ev.decision1 + 1.15), autoSeg: abs('custody', 5.9), autoOn: abs('custody', ev.mode),
    propose2: abs('custody', ev.propose2), arrive2: abs('custody', ev.propose2 + 2.3), approve2: abs('custody', ev.decision2), land2: abs('custody', ev.decision2 + 1.15),
  },
  limits: { start: abs('limits', LIMITS_START), halt: abs('limits', haltU) },
  proof: { claims: Array.from({ length: 8 }, (_, i) => abs('proof', 3.0 + i * 0.78)), done: abs('proof', 9.0) },
  ultra: { steps: [0, 0.2, 0.4, 0.62, 0.8].map((p) => abs('ultra', p * (S.ultra.t1 - S.ultra.t0) + 0.05)) },
  models: { from: abs('models', 0.5), to: abs('models', 3.8) },
  new: { shot: abs('new', 0.3), items: [0, 1, 2, 3].map((i) => abs('new', 0.8 + i * 0.28)) },
  cta: { ticks: [1.5, 2.1, 2.8, 3.4].map((u) => abs('cta', u)) },
  typing,
};
console.log(JSON.stringify(cues, null, 1));
