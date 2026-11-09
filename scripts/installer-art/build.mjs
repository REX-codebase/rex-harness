// Renders the Windows setup wizard's artwork (the sidebar on the welcome and
// finish pages, the header on the others) from the HTML in this folder, and
// writes the 24-bit BMPs the NSIS installer needs into src-tauri/installer/.
//
//   node scripts/installer-art/build.mjs
//
// Needs the dev dependencies (playwright, @fontsource/*) and python3 with
// Pillow. Set CHROMIUM_PATH to use a specific Chromium binary. The BMPs are
// committed, so a normal build does not run this; run it after changing the
// artwork or the version shown on the sidebar.

import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const out = join(root, "src-tauri", "installer");
mkdirSync(out, { recursive: true });
const version = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;

// width x height in pixels, as the NSIS pages draw them (no scaling).
const art = [
  { page: "sidebar.html", file: "sidebar.bmp", w: 164, h: 314 },
  { page: "header.html", file: "header.bmp", w: 150, h: 57 },
];
const SCALE = 4; // rendered at 4x and filtered down, so edges are smooth

const work = mkdtempSync(join(tmpdir(), "rex-installer-art-"));
const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
try {
  for (const { page: name, file, w, h } of art) {
    // the version is stamped into a copy next to the original so relative paths keep working
    const html = readFileSync(join(here, name), "utf8").replaceAll("__VERSION__", version);
    const stamped = join(here, `.stamped-${name}`);
    writeFileSync(stamped, html);
    const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: SCALE });
    await page.goto(pathToFileURL(stamped).href);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForFunction(() => [...document.images].every((i) => i.complete));
    const png = join(work, file.replace(".bmp", ".png"));
    await page.screenshot({ path: png });
    await page.close();
    execFileSync("python3", [
      "-c",
      `import sys
from PIL import Image
src, dst, w, h = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
im = Image.open(src).convert("RGB").resize((w, h), Image.LANCZOS)
im.save(dst, "BMP")  # 24-bit, uncompressed, BITMAPINFOHEADER: what NSIS accepts`,
      png,
      join(out, file),
      String(w),
      String(h),
    ]);
    console.log(`wrote ${join("src-tauri/installer", file)} (${w}x${h})`);
  }
} finally {
  await browser.close();
}
