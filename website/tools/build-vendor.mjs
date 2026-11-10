#!/usr/bin/env node
// Builds website/vendor/three.min.js: a tree-shaken, minified ES module holding
// only the three.js exports the site imports. The page loads it through an
// import map, so the source reads like normal three.js code.
//
//   npm i --prefix /tmp/rex-site-deps three@0.187.0 esbuild
//   node website/tools/build-vendor.mjs /tmp/rex-site-deps
//
// The three.js version is pinned on purpose: a landing page used as a benchmark
// has to render the same way next year as it does today.

import { readdirSync, readFileSync, writeFileSync, statSync, mkdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { gzipSync } from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const site = resolve(here, '..');
const deps = resolve(process.argv[2] || join(site, 'node_modules'));
const require = createRequire(join(deps, 'noop.js'));
const esbuild = require('esbuild');
const threePkg = JSON.parse(readFileSync(join(deps, 'node_modules/three/package.json'), 'utf8'));

const walk = (dir) =>
  readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : p.endsWith('.js') ? [p] : [];
  });

const names = new Set();
for (const file of walk(join(site, 'js'))) {
  const src = readFileSync(file, 'utf8');
  for (const m of src.matchAll(/import\s*\{([^}]*)\}\s*from\s*['"]three['"]/g)) {
    for (const part of m[1].split(',')) {
      const n = part.trim().split(/\s+as\s+/)[0].trim();
      if (n) names.add(n);
    }
  }
}
if (!names.size) throw new Error('no imports from "three" found under website/js');

const entry = `export { ${[...names].sort().join(', ')} } from 'three';\n`;
const out = await esbuild.build({
  stdin: { contents: entry, resolveDir: join(deps, 'node_modules/three'), loader: 'js' },
  bundle: true,
  format: 'esm',
  minify: true,
  legalComments: 'none',
  target: 'es2020',
  write: false,
  nodePaths: [join(deps, 'node_modules')],
  banner: { js: `/* three.js r${threePkg.version.split('.')[1]} (${threePkg.version}), MIT, https://threejs.org. Tree-shaken to the ${names.size} exports the site uses. Rebuild: website/tools/build-vendor.mjs */` },
});
mkdirSync(join(site, 'vendor'), { recursive: true });
const code = out.outputFiles[0].text;
writeFileSync(join(site, 'vendor/three.min.js'), code);
writeFileSync(join(site, 'vendor/LICENSE-three.txt'), readFileSync(join(deps, 'node_modules/three/LICENSE'), 'utf8'));
console.log(`three@${threePkg.version}: ${names.size} exports, ${(code.length / 1024).toFixed(0)} KB min, ${(gzipSync(code).length / 1024).toFixed(0)} KB gzip`);
