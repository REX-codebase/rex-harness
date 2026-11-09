# Buttons

`src/buttons.css` is the one place a button's look is decided. It loads last
and replaces what used to be a dozen unrelated button classes, each with its
own radius, border and dim grey text.

## Roles

| Role | Used for | Look |
| --- | --- | --- |
| Primary | the one thing to do next: Run, Approve, Send answer, setup and stage actions | accent gradient face, top highlight, glow; ink text |
| Secondary | everything else you can press: Deny, Cancel, Reset, model picker, options, chips | raised face, visible edge, full-contrast text |
| Quiet | links and text toggles | unchanged |

States are the same everywhere: hover brightens and lifts 1px, active presses
in, `:focus-visible` draws a 2px ring outside the edge, and disabled is
`not-allowed`. A disabled primary keeps its own colour, dimmed (tinted text, a
faint edge), so it reads as waiting for input instead of broken.

## Tokens

Set on the shell, so Ultra and Fast change every button without extra rules:

- `--btn-r`: corner radius. 10px standard, 7px in Fast, 4px in Ultra.
- `--btn-accent` / `--btn-ink`: the primary face. Defaults to the shell accent;
  approval cards override it (amber for a command, blue for a plan).
- `--btn-edge`, `--btn-edge-hot`, `--btn-face`, `--btn-face-hot`,
  `--btn-lit`, `--btn-drop`, `--btn-lift`: the secondary face and its shadows.

## Mode switches

Fast and Ultra are switches, so they say on or off: a small lamp in the corner
lights (accent for Fast, champagne for Ultra), the face warms, and Ultra's
glyph is always a star (dim and a quarter-turn out of true when off, locked in
when on). The lamp sits in the corner so it costs no width; the top bar is
already tight, and `verification/buttons` checks it fits down to 390px.

## What this file does not own

Entrance animations, ripples, the Run button's sheen and busy state, and
spring transforms live in `motion-ui.css`. The options button keeps that
file's tilt. Under Reduce, and on devices that cannot hover, the lift and the
spring are removed (`docs/motion.md`).
