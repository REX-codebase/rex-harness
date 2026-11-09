# Motion preference

Settings > Appearance > Motion has three modes. The in-app choice always wins
over the OS; System is the only mode that delegates to it.

- **System** follows the device's `prefers-reduced-motion` setting, live. If
  the OS setting flips while the app is open, the interface updates without a
  reload (`useSyncExternalStore` over a `matchMedia` change listener).
- **Reduce** is a calm but fully usable interface: animations and transitions
  are instant, ambient effects (Ultra blades, Fast iris, shimmer skeletons,
  pulsing state dots) are removed, and timed mock sequencing shortens. Every
  control still works; nothing is only hidden.
- **Full** is the complete approved motion design. It applies even when the
  OS asks for reduced motion, because the user explicitly chose it here.

## Implementation

- `src/data/motion.ts` owns the contract: `MotionPref`, storage under the
  `rex-harness-motion` localStorage key (this device only), `resolveReduced`,
  and the live OS subscription.
- The shell carries `motion-<mode>` and, when resolved, `reduced-fx`. All
  reduced-motion suppression is scoped to those classes. The raw
  `@media (prefers-reduced-motion: reduce)` block applies only under
  `.motion-system`, so an explicit Reduce or Full choice is never overridden
  by the OS.
- JS-driven timing (hero settle, Fast engage/disengage, mock and browser
  turn sequencing) reads the same resolved `reduced` flag.

## Tests

- `npm test` (vitest): parsing, storage fallback, the System/Reduce/Full
  resolution matrix, and the OS subscription including the legacy
  `addListener` path.
- `node verification/motion-settings/verify-motion.mjs` (Playwright, real
  Chromium): persistence across reload, Reset to defaults, live OS flips in
  System mode via `page.emulateMedia`, Full overriding an OS-level reduce
  (hero animation 380ms, Fast transition overlay renders), Reduce calming
  every animation layer (hero 0.01ms, no Fast overlay, instant model menu)
  while controls stay usable. Screenshots and `checks.json` land beside the
  script.

## Depth and motion layer

`src/motion-ui.css` (loaded after `index.css`) and `src/components/Fx.tsx`
add depth and choreography on top of the contract above. They follow the same
rules, plus a few of their own.

What it does, and why each piece exists:

- **Backdrop** (`<Ambient>`): a lit horizon, a faint ledger grid, a cursor halo
  that trails the pointer, and grain. It tints by the shell's `data-run`
  value (`idle | working | waiting | done | blocked`, from `runPhase()` in
  `Fx.tsx`): breathing quickens while working, turns amber when a human is
  needed, and blooms green once on completion. Only `promoted` Ultra runs
  and `completed` agent runs count as done; failures never tint green.
- **Sliding pill** (`useSlideIndicator`): the tab strip, Build/Plan and the
  Motion setting share one pill that glides with a spring, drawn as a
  `::before` from `--ind-*` variables. Without layout (jsdom) it stays off and
  the plain per-button highlight applies.
- **Field light**: `[data-glow]` elements get a cursor-following highlight; a
  focused field plays one sweep around its border, and a busy field orbits.
- **State rail**: nodes light, links fill, the check draws. A link only fills
  when the stage after it is actually reached. The active stage's link carries
  a travelling pulse that shows activity, never progress.
- **Receipt**: the result summary wipes in, evidence rows arrive in order with
  drawn checks, the receipt ID stamps. Entrances are by mount, so polling
  re-renders never replay them (timeline row keys are index-stable).
- **Approvals and plans**: a slow breathing edge while a human is needed
  (amber for tools, blue for plans).

Rules for adding motion here:

1. Entrances use `animation-fill-mode: backwards`, never `both`/`forwards`, so
   the element returns to its normal styles and hover/active transforms keep
   working. (A verified regression risk: a forwards-filled `transform` would
   freeze `:active`.)
2. Reduce zeroes animation **and transition delays** (`.reduced-fx *`), because
   a delayed `backwards` entrance would otherwise sit invisible for its whole
   delay. Looping and pointer effects are removed outright under `.reduced-fx`.
   Every shell that renders animated content must carry `motion-<mode>` and
   `reduced-fx`; the first-run gate now does.
3. Springs are real damped-spring step responses emitted as CSS `linear()`
   (`--spring-soft | --spring-snappy | --spring-bouncy`, settle times in
   `--dur-*`), with cubic-bezier fallbacks for engines without `linear()`.
4. Gradient-clipped text is wrapped in `@supports (background-clip: text)` so
   an unsupported engine shows plain text, not invisible text.
5. Page entrances use `:where()` so they carry zero specificity and any
   component's own animation (for example the setup panel's attention shake)
   still wins. The entrance is excluded under `.fast-on`, so Fast mode is
   unchanged and keeps its existing rules.
6. Generated content that is only decoration uses the `content: "x" / ""`
   alt form so it is not read out as part of a control's name.

## Layer tests

- `src/components/Fx.test.tsx` (vitest): indicator geometry and its no-layout
  fallback, the run-phase mapping (including that failed Ultra endings are
  never "done"), pointer tracking, Ambient being decorative and static under
  Reduce, and the state rail's stage/link semantics.
- `node verification/motion-ui/verify-motion-ui.mjs [baseUrl]` (Playwright,
  real Chromium against `vite preview`): samples computed styles over time to
  prove the pill springs and lands on its target, the headline staggers and
  ends opaque, Reduce leaves nothing hidden and no delays (in-app, first-run
  gate and OS-level), `:active` survives entrance animations, the pointer
  light tracks, the rail lights one active stage without filling ahead, and
  the backdrop reads working/done. Exits non-zero on any failure. Screenshots
  and `checks.json` land beside the script.
