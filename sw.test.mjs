import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';

// The precache list in sw.js is hand-maintained. A listed file that is missing breaks the whole
// install (addAll rejects), a referenced file that is missing from the list breaks offline use.
const sw = readFileSync('sw.js', 'utf8');
const list = sw.match(/const ASSETS = \[([\s\S]*?)\];/)[1];
const assets = [...list.matchAll(/'\.\/([^']+)'/g)].map(m => m[1]);
const cached = new Set(assets);

// No duplicates, and every entry exists on disk.
assert.equal(cached.size, assets.length, 'duplicate ASSETS entries');
for (const path of assets) assert(existsSync(path), `precache entry not found: ${path}`);

// Static and dynamic imports of precached modules.
for (const path of assets.filter(p => p.endsWith('.js'))) {
  for (const m of readFileSync(path, 'utf8').matchAll(/(?:\bfrom\s*|import\s*\()\s*['"](\.[^'"]+)['"]/g)) {
    const dep = m[1].replace(/^\.\//, '');
    assert(cached.has(dep), `${path} imports ${dep}: missing from ASSETS`);
  }
}

// Assets referenced by the stylesheet: url(...) is relative to the root like the precache.
for (const m of readFileSync('style.css', 'utf8').matchAll(/url\((['"]?)([^)'"]+)\1\)/g)) {
  assert(cached.has(m[2]), `style.css loads ${m[2]}: missing from ASSETS`);
}

// Local references in the page: manifest, icons, stylesheet and entry module.
for (const m of readFileSync('index.html', 'utf8').matchAll(/(?:href|src)="([^"#][^":]*)"/g)) {
  if (/^(https?:)?\/\//.test(m[1])) continue;
  assert(cached.has(m[1]), `index.html loads ${m[1]}: missing from ASSETS`);
}

// Manifest icons are precached; install previews are fetched by the browser on demand and must
// only exist on disk.
for (const file of assets.filter(p => p.startsWith('manifest.') && p.endsWith('.webmanifest'))) {
  const manifest = JSON.parse(readFileSync(file, 'utf8'));
  for (const icon of manifest.icons) assert(existsSync(icon.src), `${file}: icon not found: ${icon.src}`);
  for (const shot of manifest.screenshots ?? []) assert(existsSync(shot.src), `${file}: screenshot not found: ${shot.src}`);
}
