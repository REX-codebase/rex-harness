# REX Harness launch film

A 71-second, 1080p, 30 fps launch film for REX Harness 0.1.2, built from the project itself.

- **The picture is the website's real 3D scene.** [`tools/render-plates.mjs`](tools/render-plates.mjs)
  loads `website/` in headless Chromium and renders its WebGL scene frame by frame at the highest quality tier,
  with a scripted camera. The scene code is not modified.
- **The type and panels are HTML**, drawn in the site's own tokens and fonts
  ([`src/comp.html`](src/comp.html), [`src/comp.css`](src/comp.css), [`src/comp.js`](src/comp.js)), every frame a pure
  function of time.
- **The sound is synthesized** ([`tools/synth_audio.py`](tools/synth_audio.py)): no samples, no stock music, so there is
  nothing to license. It is cut to the same timeline as the picture.

## What it shows, and where each claim comes from

| Time | Beat | Source |
| --- | --- | --- |
| 0:00 | "Supervise it? Budget it? Verify it?" | the tagline in `README.md` |
| 0:05 | *Vast power, held in orbit.* | `website/index.html` hero |
| 0:11 | **Custody**: a proposal flies to the gate, you approve, it lands in the disposable copy; then **Auto** | `README.md`, `docs/autonomous-mode.md`, the site's approval demo |
| 0:23 | **Limits**: four rings, four limits; a run stops at the first one | the site's limits demo, driven for real |
| 0:34 | **Proof**: `rex verify`, `rex replay`, a receipt's claims | `docs/cli.md`, `README.md` |
| 0:44 | **Ultra**: contract, candidates, adversary, verifier, promotion | `docs/ultra-mode.md` |
| 0:53 | **Models**: thirteen routes | `README.md` |
| 0:57 | **What's new in 0.1.2** and the real interface | `CHANGELOG.md`, `docs/images/ui-home.png` |
| 1:02 | Install, links, and the honest status line | `INSTALL.md`, `README.md` |

Anything that is a demonstration rather than a recording is labelled **Illustration** on screen, as it is on the site.
The interface screenshot is the real one and is captioned as sample data, as in the README. The 3D limit rings, the
custody gate and the meters are the site's own scene and its own demo logic: the limits shot clicks the page's real
*Start a run* button and records the page's real meter state (sped up 1.8x so a run fits in one shot).

## Build it

```bash
cd video
npm install            # Playwright (or have it installed globally)
npx playwright install chromium
pip install numpy scipy
./build.sh             # -> rex-harness-launch.mp4 and a poster frame
```

A full render takes roughly 40 minutes on four CPU cores with no GPU: the 3D scene is software-rendered at about one
second per 1080p frame. With a GPU it is much faster. Intermediates go to `out/` (git-ignored, a few GB); set
`OUT=/elsewhere` to move them.

Each stage can be run on its own:

```bash
node tools/render-plates.mjs --out out/plates --shots custody          # one shot's 3D
node tools/render-comp.mjs  --plates out/plates --out out/review --every 30   # a contact sheet of the whole film
node tools/render-comp.mjs  --plates out/plates --out out/review --list 462,526 # exact frames
node tools/print-cues.mjs --plates out/plates > out/cues.json
python3 tools/synth_audio.py out/cues.json out/score.wav
```

## Changing it

- **Timing and order**: `src/script.mjs` (the shot list). The compositor, plates and audio all read it.
- **What the 3D does in each shot**: `src/plates.mjs` (chapter, progress, camera overrides, scripted events).
- **Copy and layout**: `src/comp.html`, `src/comp.css`; choreography in `src/comp.js`.
- **Sound**: `tools/synth_audio.py`, driven by `tools/print-cues.mjs`. If you move a beat in `comp.js` that makes a
  sound (a click, a typed line), move its cue in `print-cues.mjs` too.

Plates are the slow part. Edits to copy, layout, motion and audio only need `render-comp` and the audio step again.

## How the 3D renders deterministically

`render-plates.mjs` replaces `requestAnimationFrame` and `performance.now` with a virtual clock, seeds
`Math.random`, and reads each frame straight from the canvas in the same task as the draw. One call advances the
site's own render loop by exactly one 1/30 s frame. Two lines of the site's `ui.js` are patched *in memory* so the DOM
demos do not fire their own events over the choreography and the limits demo runs 1.8x faster; the file on disk is
untouched, and the render fails loudly if those lines change upstream.
