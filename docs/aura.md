# REX aura

One idea runs through the interface: the REX mark gives off light, and that
light is the colour of the surface it falls on. Silver and violet in the
standard tier, champagne in Ultra. All of it is decoration. It never carries
state, claims a capability, or takes pointer input.

## The typed wordmark

The top-left mark is live text (`RexWordmark`, set in Orbitron), not the traced
bitmap. Three glyphs share one metallic fill with the same hard split as the
original logo: bright upper half, cooler lower half. On load the letters type
in one at a time, a caret hops along behind them and blinks out, a rail draws
under the mark, and "Harness" is written after it. A sheen then crosses the
letters every few seconds and a halo breathes behind them.

- Accessible name is `REX` (the wrapper is `role="img"`); the glyph spans,
  halo and sheen copy are `aria-hidden`.
- Colours are registered custom properties (`--rex-top`, `--rex-bot`,
  `--rex-glow`) on the shell, so the metal glides from silver to champagne
  when `.ultra-on` is set. The headline's inline "REX" reads the same values.
- Toggling Ultra does not remount the mark. Only the glyph run is re-keyed so
  it types again, delayed to land as the transition clears. The delay is held
  for the life of each run: changing `animation-delay` under a finished
  animation would snap the letters back to their hidden first frame.

## Ambient light

- `AuraLayer`: two sets of soft blobs (violet, champagne) that crossfade with
  `.ultra-on`, a pointer-following glow and fine grain. The first blob sits
  behind the wordmark so the mark's light bleeds into the page.
- `useAuraPointer` writes `--mx/--my/--mo` on the shell and `--px/--py` on the
  surface under the cursor (`GLOW_SELECTOR`), one write per animation frame, as
  CSS variables only (no React state). Touch is ignored.
- Composer and follow-up fields get a spotlight on hover or focus and, while
  focused, a thread of light travelling the border.

## Ultra transition

`UltraTransition` stages the tier change as one shot, both ways: a shockwave
leaves the control that was pressed (its centre, so keyboard activation works),
seven slats at the wordmark's own -14 degree lean shutter across, and the tier
name types itself in the gap (`ULTRA` in champagne, `STANDARD` in violet on the
way out). The state flips at t=0; the slats hide the change. Total ~2s.

## Reduced motion

Under `.reduced-fx` (Settings > Motion > Reduce, or System with the OS
preference) the look stays and the movement goes: glyphs are fully drawn, no
caret, sheen, halo breathing, drifting blobs, cursor glow, travelling border
or transition overlay. See `docs/motion.md`.
