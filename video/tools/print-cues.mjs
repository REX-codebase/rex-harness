#!/usr/bin/env node
// Prints the cue sheet as JSON: every moment the picture and the sound must agree on.
//   node tools/print-cues.mjs --plates DIR > out/cues.json
//
// Times are film seconds. Moments inside the legacy footage (the limits, gate, proof and ultra shots,
// which are re-cut from plates rendered earlier) are mapped through the shot's u0. The moment the real
// limits demo hit its token limit is read from the plate's recorded tracks, so the stop lands on
// the frame where the rings freeze.
import fs from 'node:fs';
import path from 'node:path';
import { SHOTS, DURATION } from '../src/script.mjs';
import { CUSTODY_EVENTS, LIMITS_START, RING_LOCKS, RING_FLIGHT, REVEAL_FLARES, TENSION_FLARES } from '../src/plates.mjs';

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1]] : null)).filter(Boolean));
const S = Object.fromEntries(SHOTS.map((s) => [s.id, s]));
const abs = (id, u) => +(S[id].t0 + u).toFixed(3);                       // local time in a new-footage shot
const leg = (id, pu) => +(S[id].t0 + pu - S[id].plate.u0).toFixed(3);    // plate time in a legacy-footage shot

const ev = Object.fromEntries(CUSTODY_EVENTS.map((e) => [`${e.type}${e.id ?? ''}`, e.at]));
let haltPlate = LIMITS_START + 4.7;
if (args.plates) {
  const f = path.join(args.plates, 'limits/tracks.json');
  if (fs.existsSync(f)) {
    const tr = JSON.parse(fs.readFileSync(f, 'utf8'));
    const i = tr.findIndex((x) => x && x.run && x.run.halted);
    if (i >= 0) haltPlate = i / 30;
  }
}
const halt = leg('limits', haltPlate);
const approve = leg('gate', ev.decision1);

const cues = {
  duration: DURATION,
  shots: Object.fromEntries(SHOTS.map((s) => [s.id, { t0: s.t0, t1: s.t1 }])),
  open: { beat: 1.16 }, // heartbeat period, seconds
  reveal: { flares: REVEAL_FLARES.map((f) => ({ at: abs('reveal', f.at), a: f.a })) },
  tension: { flares: TENSION_FLARES.map((f) => ({ at: abs('tension', f.at), a: f.a })) },
  harness: { locks: RING_LOCKS.map((u) => abs('harness', u)), flight: RING_FLIGHT, names: ['STEPS', 'TOOL CALLS', 'TOKENS', 'TIME'], flags: ['--max-steps', '--max-tool-calls', '--max-tokens', '--timeout-secs'] },
  limits: { start: leg('limits', LIMITS_START), halt },
  gate: { propose: leg('gate', ev.propose1), arrive: leg('gate', ev.propose1 + 2.3), approve, land: leg('gate', ev.decision1 + 1.15) },
  proof: { scanFrom: leg('proof', 2.3), scanTo: leg('proof', 9.2), done: leg('proof', 9.0), claims: Array.from({ length: 8 }, (_, i) => leg('proof', 3.0 + i * 0.78)) },
  ultra: { steps: [0, 0.2, 0.4, 0.62, 0.8].map((p) => leg('ultra', p * (S.ultra.t1 - S.ultra.t0) + 0.05)) },
  models: { from: abs('models', 0.5), to: abs('models', 3.8) },
  finale: { title: abs('finale', 0.9), tag: abs('finale', 2.8), end: abs('finale', 9.2) },
  // typed commands in the HUD [start, characters, chars/s]. Keep in sync with src/comp.js.
  typing: [
    [leg('proof', 2.2), 24, 22], [leg('proof', 5.2), 24, 22],
  ].map(([at, n, cps]) => ({ at: +at.toFixed(3), n, cps })),
  // The narration. `at` is when the line starts; narrate.py measures how long each takes.
  narration: [
    { id: 'v01', at: 1.3, text: 'It can read your entire codebase in a second.' },
    { id: 'v02', at: 6.9, text: 'Rewrite it in a minute.' },
    { id: 'v03', at: 9.4, text: 'It never tires. It never hesitates.' },
    { id: 'v04', at: 13.6, text: 'And it will do exactly what you ask.' },
    { id: 'v05', at: 16.3, text: 'Or exactly what it thinks you meant.' },
    { id: 'v06', at: 19.7, text: 'Power like that needs a harness.' },
    { id: 'v07', at: abs('harness', 8.0), text: 'Hard limits. Not suggestions.' },
    { id: 'v08', at: +(halt - 4.3).toFixed(2), text: 'When it reaches one,' },
    { id: 'v09', at: +(halt + 0.15).toFixed(2), text: 'it stops. And tells you why.' },
    { id: 'v10', at: 38.3, text: 'By default, nothing touches your code until you say so.' },
    { id: 'v11', at: +(approve - 0.1).toFixed(2), text: 'Every change is your call.' },
    { id: 'v12', at: 45.1, text: "When it finishes, it doesn't tell you it worked." },
    { id: 'v13', at: 49.3, text: 'It proves it.' },
    { id: 'v14', at: 50.9, text: 'A signed receipt. Checked offline.' },
    { id: 'v15', at: 55.0, text: 'And when it really matters, let them compete.' },
    { id: 'v16', at: 63.3, text: 'Your models. Your machine.' },
    { id: 'v17', at: abs('finale', 0.9), text: 'REX Harness.' },
    { id: 'v18', at: abs('finale', 2.9), text: 'Vast power. Held in orbit.' },
  ],
};
console.log(JSON.stringify(cues, null, 1));
