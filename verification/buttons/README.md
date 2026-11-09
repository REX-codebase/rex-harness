# Button system verification

Run against a production build:

```bash
npm run build && npx vite preview --port 4173 &
node verification/buttons/verify-buttons.mjs
```

`checks.json` records each check (24 at the time of writing). The script
samples computed styles in real Chromium, so it proves the properties that make
buttons read as buttons instead of relying on a screenshot:

- a disabled primary stays recognisably the primary (tinted text, a 1px edge,
  `not-allowed`), not a grey slab, and typing visibly arms it;
- secondary buttons have a raised face, a visible edge and a 40px target;
  "Connect a model" is a bordered 44px control that asks for attention;
- inactive tabs meet 4.5:1 contrast;
- hover lifts, active presses, the options button keeps its tilt, and both
  Reduce and `(hover: none)` remove the movement;
- Fast and Ultra each light a corner lamp, the corner radius follows the tier
  (10px, 7px Fast, 4px Ultra), and Ultra's glyph is a star in every state;
- the top bar fits at 1280, 820, 640 and 390px, Ultra off and on;
- a focused button shows a 2px ring.

Run against a build without `src/buttons.css` it fails 13 of the checks, which
is the point: each one pins a value the old styles had wrong.

`before-after.jpg` pairs each state (before on top, after below): the header
switches, the composer with Run disabled and armed, an approval card, and a
question card. In the old question card "Send answer" looked fully clickable
while it was disabled; it now reads as waiting.
