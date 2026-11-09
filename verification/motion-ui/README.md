# Depth and motion layer verification

Run against a production build:

```bash
npm run build && npx vite preview --port 4173 &
node verification/motion-ui/verify-motion-ui.mjs
```

`checks.json` records each check (20 at the time of writing). Screenshots:

| File | State |
| --- | --- |
| `01-gate.png` | First-run operator gate with the backdrop |
| `02-hero.png` | Task surface after the headline has arrived |
| `03-plan-mode.png` | Plan mode: the pill slid to Plan and took the plan tint |
| `04-run-working.png` | Sample run mid-flight: active node, travelling link, busy follow-up |
| `05-run-done.png` | Sample run complete: drawn checks, receipt, history |
| `06-approval-gallery.png` | Live-run approval card (from `dev/gallery`, `?s=approval`) |

Stills cannot show motion; the script's value is in the sampled computed
styles (pill travel and overshoot, headline stagger, Reduce with zero delays).
The fixed backdrop shows only at the top of the full-page capture in `05`;
that is how Playwright stitches fixed layers, not a layout fault.
