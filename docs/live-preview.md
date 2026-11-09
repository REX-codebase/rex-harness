# Live app preview runtime

`crates/rex-preview` defines the native trust boundary for UI projects created
inside the selected workspace. It detects static HTML, Vite, React Scripts,
Next.js, Astro, and SvelteKit from bounded manifest snapshots; creates argv-only
loopback launch plans in a reserved port range; refuses non-loopback or
wrong-port URLs; and exposes normalized pointer, keyboard, text, scroll, and
viewport actions. Screenshots, DOM/accessibility snapshots, console errors,
failed requests, viewports, gates, accepted iterations, and rejected diffs use a
bounded typed evidence protocol.

Completion is evidence-driven. A run can pass only when startup, console,
network, desktop, mobile, keyboard, accessibility, screenshot, test, and build
gates all pass. There is no self-score completion API. Cancellation is terminal
and tells the native supervisor to kill the process tree.

The current milestone contains the Rust policy/lifecycle, Tauri command shapes,
frontend bridge, visible agent-cursor/iteration/evidence UI, and adversarial unit
tests. Native preview process spawning and browser-engine capture are still open;
the Tauri bridge truthfully leaves a newly started preview in `starting` until
that supervisor lands.
