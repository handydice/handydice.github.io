import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';

// Offline the app only has what sw.js precaches: every module, icon, sound and thumbnail must be listed.
const root = new URL('.', import.meta.url);
const assets = [...readFileSync(new URL('sw.js', root), 'utf8').matchAll(/'\.\/([^']*)'/g)].map(m => m[1]).filter(Boolean);
const files = dir => readdirSync(new URL(dir, root)).filter(name => !name.startsWith('.')).map(name => dir + name); // skips .DS_Store
const expected = [
  ...files('').filter(f => /\.(js|css|html|png|svg|webmanifest)$/.test(f) && f !== 'sw.js'),
  ...files('icons/'), ...files('sounds/'), ...files('backgrounds/thumbs/'),
  ...files('backgrounds/').filter(f => f.endsWith('.webp')),
  ...readdirSync(new URL('skins/', root)).flatMap(skin => files(`skins/${skin}/`)),
];

assert.deepEqual(expected.filter(f => !assets.includes(f)), [], 'missing in sw.js ASSETS');
assert.deepEqual(assets.filter(f => !existsSync(new URL(f, root))), [], 'listed in sw.js ASSETS but missing on disk');

console.log('service worker assets ok');
