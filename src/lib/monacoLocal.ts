// The bundled Monaco: the editor core, syntax highlighting for the usual
// languages, and JSON (with its worker). Heavier language services (TypeScript,
// CSS and HTML validation) are left out on purpose: they add megabytes for
// diagnostics this tab does not show.
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
import "monaco-editor/esm/vs/editor/edcore.main.js";
import "monaco-editor/esm/vs/language/json/monaco.contribution.js";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker.js?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker.js?worker";

// Syntax highlighting for every language Monaco ships a grammar for. Each of
// these is only a small registration stub; a language's tokenizer is fetched the
// first time a file of that language is opened.
import.meta.glob("/node_modules/monaco-editor/esm/vs/basic-languages/*/*.contribution.js", { eager: true });

(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker: (_id: string, label: string) => (label === "json" ? new JsonWorker() : new EditorWorker()),
};

loader.config({ monaco });
