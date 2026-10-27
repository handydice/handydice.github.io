import { createHash } from 'node:crypto';
import { cp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const mode = process.argv.includes('--dev') ? '--dev' : '--release';
await rm('dist', { recursive: true, force: true });
const result = spawnSync('wasm-pack', ['build', '--target', 'web', '--out-dir', 'dist/pkg', '--no-typescript', mode], { stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
await rm('dist/pkg/.gitignore', { force: true });
await rm('dist/pkg/README.md', { force: true });
await rm('dist/pkg/package.json', { force: true });
const assets = ['backgrounds', 'icons', 'skins', 'sounds', 'screenshots', 'style.css', 'index.html', 'LICENSE', 'icon.svg', 'icon-192.png', 'icon-512.png', 'icon-maskable-512.png', 'apple-touch-icon.png'];
for (const lang of ['en', 'de', 'fr', 'es', 'pt', 'it', 'nl', 'pl']) assets.push(`manifest.${lang}.webmanifest`);
for (const path of assets) await cp(path, join('dist', path), { recursive: true });
async function files(dir) {
  const paths = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    paths.push(...(entry.isDirectory() ? await files(path) : [path.replace(/^dist\//, '')]));
  }
  return paths;
}
const precache = (await files('dist')).filter(path => path !== 'sw.js' && !path.startsWith('screenshots/')).sort();
const hash = createHash('sha256');
for (const path of precache) hash.update(await readFile(join('dist', path)));
const worker = await readFile('sw.js', 'utf8');
await writeFile('dist/sw.js', worker.replace('__CACHE_VERSION__', hash.digest('hex').slice(0, 16)).replace('/* __PRECACHE__ */', JSON.stringify(['./', ...precache.map(path => `./${path}`)])));
console.log(`Built ${mode === '--release' ? 'release' : 'development'} WASM app in dist/ (${precache.length} offline assets).`);
