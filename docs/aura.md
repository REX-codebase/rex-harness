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

The motion layer (`docs/motion.md`, `Fx.tsx`, `motion-ui.css`) owns the
backdrop, the cursor halo, the field spotlight and focus sweep, grain and the
Run button. The aura does not duplicate any of it. `AuraLayer` adds only two
soft blobs on top of that backdrop:

- one takes the wordmark's own glow colour (`--rex-glow`: silver-violet,
  champagne in Ultra, gliding between), anchored behind the mark so its light
  bleeds into the page;
- one takes `--amb`, the run-state tint the backdrop already uses, so a run
  that is waiting or blocked tints it amber like the rest.

The Ultra control warms on hover and holds a static glow while on. It is
static on purpose: the motion layer gives that control an entrance animation,
and a looping glow would replay the entrance each time Ultra turns off.

## Ultra transition

`UltraTransition` stages the tier change as one shot, both ways: a shockwave
leaves the control that was pressed (its centre, so keyboard activation works),
seven slats at the wordmark's own -14 degree lean shutter across, and the tier
name types itself in the gap (`ULTRA` in champagne, `STANDARD` in violet on the
way out). The state flips at t=0; the slats hide the change. Total ~2s.

## Reduced motion

Under `.reduced-fx` (Settings > Motion > Reduce, or System with the OS
preference) the look stays and the movement goes: glyphs are fully drawn, no
caret, sheen, halo breathing, drifting blobs or transition overlay. The
motion layer's own reduce rules (zeroed durations and delays) apply on top.
See `docs/motion.md`.
