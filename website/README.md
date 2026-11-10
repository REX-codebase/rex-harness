# REX Harness website

The landing page for REX Harness, published to GitHub Pages at
<https://rex-codebase.github.io/rex-harness/>.

It is a static site: HTML, CSS and ES modules with a pinned, tree-shaken copy of
three.js. There is no build step, no framework, no analytics and no third-party
request. Open `index.html` through any static file server.

```bash
python3 -m http.server 8080 --directory website      # then open http://localhost:8080
node website/tools/check.mjs                           # the checks CI runs before every deploy
node website/tools/verify.mjs                          # real-browser verification (needs Playwright: `npm ci`)
```

`verify.mjs` drives the page the way visitors do and asserts on the result: desktop, a
1366×768 laptop and a 1280×720 window (no panel may clip), a portrait phone (menu
opens and closes), no WebGL, reduced motion, GPU context loss and recovery, and the two
demos clicked through end to end (Approve, Reject, the Auto confirmation whose default
focus must be *Keep asking me*, a run that stops at its first limit). Every scenario also
fails on console errors, three.js warnings, failed requests, third-party requests and
horizontal overflow, and the desktop run rebuilds the scene repeatedly to prove it does
not leak GPU resources. It runs on software GL when there is no GPU, so frames are slow
but real; it checks correctness, not speed (use `?hud` on real hardware for that).

## The idea

REX Harness holds a powerful, unpredictable thing (a language model with tools) inside
rules it cannot talk its way out of. The page makes that literal. The product is a
**cosmic entity**: a luminous core held in a precision-machined gyroscopic harness.

| In the scene | In the product |
| --- | --- |
| The **core** and its particle belts | The model working, and its activity |
| Four machined **rings** with LED arcs | The four hard limits: steps, tool calls, tokens, time |
| The **gate** and the voxel **disposable copy** | Approvals, and the sandbox copy of your folder |
| **Crystals** on a helix, a verification front climbing them | Ed25519-signed receipts, `rex verify` / `rex replay` |
| Four **candidate worlds**, an adversary, a sweep, one survivor | Ultra: contract, candidates, adversary, verifier, promotion |
| **Champagne** replacing silver-violet in Ultra | The product's own Ultra tier colour change (`src/aura.css`) |
| **13 stars** linked to the entity | The model routes REX can connect |
| Light **bending** around the core | Decoration only: real thin-lens gravitational lensing of the sky |

Every product claim on the page is taken from the repository: `README.md`,
`docs/cli.md`, `docs/autonomous-mode.md`, `docs/ultra-mode.md`, `CHANGELOG.md`. The
two interactive demos and the Ultra choreography are labelled *illustration*.

## Layout

```
index.html            the page (semantic HTML; all copy lives here)
css/site.css          design system: tokens from the product, panels, demos, responsive, print
js/main.js            entry: capability check, boot, graceful fallback
js/core.js            scroll → chapter weights/progress, shared state, event bus (no three.js)
js/ui.js              nav, reveal, copy buttons, tabs, the two demos (no three.js)
js/vitals.js          "Page vitals": the page measures itself
js/scene/             the WebGL scene, loaded after first paint
  stage.js            renderer, loop, quality governor, context-loss fallback
  quality.js          tiers + frame-time governor        post.js   HDR bloom + grade, written for this page
  sky.js              baked, lensed nebula + 16k stars   core.js   plasma core + corona
  rings.js            the harness                        flow.js   200k GPU particles, zero CPU per particle
  custody.js          gate, voxel copy, packets          limits.js live ring labels
  proof.js            receipt ledger                     ultra.js  the Ultra diagram, scrubbed by scroll
  constellation.js    model routes                       rig.js    camera poses, off-axis lens shift
vendor/three.min.js   three.js r187, tree-shaken to the exports the site imports
fonts/                Orbitron, Inter, JetBrains Mono (variable, latin, OFL)
img/                  real 0.1.2 interface screenshot, social card, touch icon
tools/                build-vendor.mjs (rebuild three), check.mjs (CI checks)
```

Updating three.js: `npm i --prefix /tmp/deps three@<version> esbuild`, then
`node website/tools/build-vendor.mjs /tmp/deps`. The script scans `js/` for
`import { … } from 'three'` and bundles exactly those names.

## Benchmark rubric

The page is meant to be a reference for what a harness landing page can be. These
are the bars it holds itself to, and how each is enforced or can be checked.

| Bar | What it means here | How to verify |
| --- | --- | --- |
| **Honest** | Claims link to docs; illustrations are labelled; the status block lists what is *not* verified (unsigned installer, ChatGPT sign-in not live-tested). | Read the *Honest status* card. |
| **Self-contained** | Zero third-party requests, no cookies, no analytics, fonts and three.js self-hosted. | `check.mjs` fails on any external `src`/`href`/`url()`; the vitals panel shows the live count. |
| **Light** | Compressed budgets: JS 200 KB (set from the first measurement, 191 KB, not derived from a principle), CSS 14 KB, fonts 120 KB, images 120 KB. | `check.mjs` enforces them. |
| **Progressive** | Works with no JS (static content), with no WebGL (CSS sky, stacked panels), with `prefers-reduced-motion` (still frames, draw-on-change), and when the GPU resets (falls back instead of going black). | `?nowebgl`, OS reduced-motion, `webglcontextlost`. |
| **Adaptive** | Starts at a tier guessed from the adapter and device, then a frame-time governor sheds resolution first, particles second. A pixel budget and MSAA step-down keep memory bounded. | `?q=high|medium|low` pins a tier; the vitals panel shows tier, fps, render scale. |
| **Fast to first paint** | The headline is HTML. The 3D loads after `load` and idle, so LCP is text, and static work (the nebula) is baked once into a cubemap rather than recomputed per pixel. | Vitals panel: LCP and CLS. |
| **Accessible** | Semantic landmarks, one `h1`, ordered headings, keyboard-operable demos and tabs, visible focus, live regions, 4.5:1 text, `forced-colors` and print styles. The canvas is `aria-hidden`; its information is always also in the copy. | `check.mjs` (headings, ids, alt, labels); keyboard through the page. |
| **Self-measuring** | Press `V` or use *Page vitals* in the footer: tier, fps, draw calls, triangles, adapter, bytes transferred, request count, third-party count, LCP, CLS, all read from real browser performance entries. | `?hud` opens it on load. |
| **Reproducible** | Pinned three.js, seeded star field, and a fixed-time mode for identical stills. | `?t=14&snap` freezes time and removes transitions. |

### URL switches

| Parameter | Effect |
| --- | --- |
| `?q=high\|medium\|low` | Pin a quality tier and turn the governor off |
| `?hud` | Open the vitals panel |
| `?t=<seconds>` | Freeze scene time |
| `?snap` | No transitions, no camera easing, ignition skipped (for screenshots) |
| `?motion=1` | Ignore `prefers-reduced-motion` |
| `?nowebgl` | Force the no-WebGL path |

## Hosting

`.github/workflows/pages.yml` publishes `website/` on every push to `main` that
touches it. It runs `tools/check.mjs`, stamps the current version from
`package.json` into the nav pill and structured data, and deploys. Release notes
and headlines are never auto-stamped, so the page cannot claim a version whose
changes it does not describe: update the *What's new* section when a release ships.

One-time setup: **Settings → Pages → Build and deployment → Source: GitHub Actions**.

## Credits

three.js (MIT), Orbitron, Inter and JetBrains Mono (SIL OFL), simplex noise by Ashima
Arts and Stefan Gustavson (MIT). The interface screenshot is the real 0.1.2 app.
