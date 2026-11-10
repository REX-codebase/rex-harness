// Words for the folder menu.

/**
 * REX names a run's sandbox `task-ui-<milliseconds in hex>`, which is useless to
 * read. Show when it was made instead ("Run · Nov 10, 14:03"), and fall back to
 * the folder name for anything else.
 */
export function runSandboxLabel(name: string, locale?: string): string {
  const m = /^task-ui-([0-9a-f]+)$/i.exec(name);
  if (!m) return name;
  const ms = parseInt(m[1], 16);
  // plausible dates only (2020-2100); anything else is not a timestamp
  if (!Number.isFinite(ms) || ms < 1_577_836_800_000 || ms > 4_102_444_800_000) return name;
  const when = new Date(ms).toLocaleString(locale, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  return `Run · ${when}`;
}

/** The folder a path ends in, whichever slash the platform uses. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
