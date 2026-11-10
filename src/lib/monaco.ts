// Monaco, bundled with the app.
//
// `@monaco-editor/react` loads Monaco from a CDN (cdn.jsdelivr.net) unless it
// is handed a copy, so the Editor tab and the diff in an approval needed
// internet access at run time and showed a spinner forever without it. That
// contradicts an app whose point is that it runs on your computer. This hands
// the loader the bundled copy instead.
//
// It is imported lazily, the first time an editor is about to show, so the app
// opens without paying for Monaco and tests never load it.
import { useEffect, useState } from "react";

let ready: Promise<void> | null = null;

/** Make `@monaco-editor/react` use the bundled Monaco. Resolves either way. */
export function ensureMonaco(): Promise<void> {
  if (!ready) {
    // If the bundled copy cannot load, the loader's own default still works
    // where there is a network, so this never rejects.
    ready = import("./monacoLocal").then(
      () => undefined,
      () => undefined,
    );
  }
  return ready;
}

/** True once the editor may be rendered. */
export function useMonacoReady(): boolean {
  const [ok, setOk] = useState(false);
  useEffect(() => {
    let alive = true;
    void ensureMonaco().then(() => {
      if (alive) setOk(true);
    });
    return () => {
      alive = false;
    };
  }, []);
  return ok;
}
