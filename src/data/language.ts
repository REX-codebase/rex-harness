// Which Monaco language a file gets, from its name. Shared by the editor and
// the approval diff so the two never disagree about highlighting.

const BY_EXTENSION: Record<string, string> = {
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  json: "json", jsonc: "json", rs: "rust", py: "python", go: "go", java: "java", kt: "kotlin",
  swift: "swift", rb: "ruby", php: "php", c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp",
  cs: "csharp", css: "css", scss: "scss", less: "less", html: "html", htm: "html", xml: "xml", svg: "xml",
  md: "markdown", markdown: "markdown", yaml: "yaml", yml: "yaml", toml: "ini", ini: "ini", cfg: "ini",
  sh: "shell", bash: "shell", zsh: "shell", ps1: "powershell", sql: "sql", lua: "lua", r: "r",
  dockerfile: "dockerfile", gradle: "groovy", vue: "html",
};

const BY_NAME: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "makefile",
  "cargo.lock": "ini",
  ".gitignore": "ini",
  ".env": "ini",
};

export function languageFor(path: string): string {
  const name = (path.split(/[\\/]/).pop() ?? "").toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name];
  const dot = name.lastIndexOf(".");
  if (dot <= 0 && !name.startsWith(".")) return "plaintext";
  const ext = dot < 0 ? name.slice(1) : name.slice(dot + 1);
  return BY_EXTENSION[ext] ?? "plaintext";
}
