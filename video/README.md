# REX Harness launch film

A 77-second, 2.39:1 film for REX Harness 0.1.2: a short story about power and restraint, narrated, scored and cut
entirely from the project itself.

- **The picture is the website's real 3D scene**, rendered frame by frame at the highest quality tier
  ([`tools/render-plates.mjs`](tools/render-plates.mjs)) with scripted camera, flares and ring animation
  ([`src/plates.mjs`](src/plates.mjs)). The scene code is not modified. The entity is the protagonist.
- **The narration is synthesized locally** with an open-source voice (Kokoro-82M, Apache-2.0, through sherpa-onnx;
  [`tools/narrate.py`](tools/narrate.py)).
- **The score and sound design are synthesized** ([`tools/synth_audio.py`](tools/synth_audio.py)): no samples, no stock
  music, nothing to license. Every strike and silence is cut to the picture from one cue sheet.
- **The type is sparse**: subtitles in the letterbox, a few title cards, and small readouts that carry only facts
  ([`src/comp.html`](src/comp.html), [`src/comp.css`](src/comp.css), [`src/comp.js`](src/comp.js)).

## The story

| Time | Beat | What is real |
| --- | --- | --- |
| 0:00 | Macro on a star's surface. *It can read your entire codebase in a second.* | the site's core shader |
| 0:06 | The camera pulls back: a star with no leash. *Rewrite it in a minute. It never tires.* | the unbound scene, rings hidden |
| 0:13 | It flares. *It will do exactly what you ask, or exactly what it thinks you meant.* | |
| 0:19 | *Power like that needs a harness.* Four rings lock home: **steps, tool calls, tokens, time** | the four limit rings, with their real `--max-*` flags |
| 0:29 | *When it reaches one, it stops, and tells you why.* | the site's limits demo, driven for real: its meters decide when the rings freeze |
| 0:38 | *By default, nothing touches your code until you say so.* A proposal waits at the gate; you approve | the site's approval scene, driven for real |
| 0:44 | *It proves it.* A signed receipt, checked offline | the site's proof scene; `rex verify` and `rex replay` are the real commands |
| 0:54 | *When it really matters, let them compete.* | the site's Ultra scene: contract, candidates, adversary, verifier, promotion |
| 1:03 | *Your models. Your machine.* Thirteen routes | the site's model constellation |
| 1:07 | **REX Harness.** *Vast power. Held in orbit.* | `README.md`, `website/index.html` |

Every claim is from the repository (`README.md`, `docs/cli.md`, `docs/autonomous-mode.md`, `docs/ultra-mode.md`,
`CHANGELOG.md`). The film says *by default* about approvals because Autonomous mode, new in 0.1.2, removes the
prompts for a run and none of the limits. The narration is dramatic; the product claims in it are not.

## Build it

```bash
cd video
npm install            # Playwright (or have it installed globally)
npx playwright install chromium
pip install numpy scipy sherpa-onnx
./build.sh             # -> rex-harness-launch.mp4 and a poster frame
```

A full render takes roughly an hour on four CPU cores with no GPU: the 3D scene is software-rendered at about one
second per frame. With a GPU it is much faster. Intermediates go to `out/` (git-ignored, a few GB); set `OUT=/elsewhere`
to move them. The narrator model (350 MB) downloads once from the sherpa-onnx release page.

Each stage runs on its own:

```bash
node tools/render-plates.mjs --out out/plates --shots harness            # one new shot's 3D
node tools/render-plates.mjs --legacy --out out/legacy                   # the 16:9 footage the film re-cuts
node tools/render-plates.mjs --out out/probe --probe '{"rings":"home","cam":{"dolly":0.6}}'   # one test frame
node tools/print-cues.mjs --plates out/legacy > out/cues.json
python3 -I tools/narrate.py out/cues.json out/voice/kokoro-multi-lang-v1_0 out/vo
python3 tools/synth_audio.py out/cues.json out/vo out/score.wav
node tools/render-comp.mjs --plates out/plates --legacy out/legacy --meta out --out out/review --every 48   # a contact sheet
node tools/render-comp.mjs --plates out/plates --legacy out/legacy --meta out --out out/review --list 545,833
```

## Changing it

- **Timing and order**: `src/script.mjs` (the shot list). The compositor, plates and audio all read it.
- **What the 3D does in each new shot**: `src/plates.mjs` (camera, ring lock times, core flares). Moving a ring lock
  moves its strike, flash and title together, because they are all derived from the same numbers.
- **The words**: the narration is in `tools/print-cues.mjs`; its timings come from the footage's own events (the gate
  approval, the moment the limits demo halts).
- **Look and type**: `src/comp.css`, `src/comp.js`.
- **Sound**: `tools/synth_audio.py`.

Plates are the slow part. Changes to words, type, motion and sound only need the narration, audio and composite steps again.

## How the 3D renders deterministically

`render-plates.mjs` replaces `requestAnimationFrame` and `performance.now` with a virtual clock, seeds
`Math.random`, and reads each frame from the canvas in the same task as the draw. One call advances the site's own
render loop by exactly one frame. Two lines of the site's `ui.js` are patched in memory so the DOM demos do not fire
their own events over the choreography and the limits demo runs 1.8x faster; the file on disk is untouched, and the
render fails loudly if those lines change upstream. The scene clock follows film time, so camera moves that continue
across a cut are seamless.
