// Bump on every release so installed clients precache the new files; sw.test.mjs checks ASSETS.
const CACHE = 'handydice-2d-3d-v11';
const ASSETS = [
  './', './index.html', './style.css',
  './app.js', './dice.js', './layout.js', './i18n.js', './sound.js', './state.js', './viewport.js',
  './dice3d.js', './physics3d.js', './renderer3d.js', './scene3d.js',
  './manifest.de.webmanifest', './manifest.en.webmanifest', './manifest.fr.webmanifest', './manifest.es.webmanifest', './manifest.pt.webmanifest',
  './manifest.it.webmanifest', './manifest.nl.webmanifest', './manifest.pl.webmanifest', './icon.svg', './icon-192.png', './icon-512.png',
  './apple-touch-icon.png', './icon-maskable-512.png',
  './icons/cup.svg', './icons/d6.svg', './icons/d12-facets.svg', './icons/d12-shading.svg', './icons/d20-facets.svg', './icons/d20-shading.svg', './icons/dx.svg', './icons/volume.svg', './icons/volume-off.svg',
  './backgrounds/thumbs/casino.webp', './backgrounds/thumbs/oak.webp', './backgrounds/thumbs/crafts.webp',
  './backgrounds/thumbs/bluewood.webp', './backgrounds/thumbs/terracotta.webp',
  './backgrounds/casino.webp', './backgrounds/oak.webp', './backgrounds/crafts.webp',
  './backgrounds/bluewood.webp', './backgrounds/terracotta.webp',
  './sounds/throw.mp3', './sounds/shuffle.mp3',
  './sounds/contact-table.wav', './sounds/contact-die.wav',
  './skins/opal/face.webp', './skins/opal/pips.webp', './skins/wood/face.webp', './skins/wood/pips.webp',
  './skins/galaxy/face.webp', './skins/galaxy/pips.webp', './skins/jade/face.webp', './skins/jade/pips.webp',
  './skins/amber/face.webp', './skins/amber/pips.webp', './skins/terrazzo/face.webp', './skins/terrazzo/pips.webp',
  './skins/ruby/face.webp', './skins/ruby/pips.webp', './skins/steel/face.webp', './skins/steel/pips.webp',
  './skins/prism/face.webp', './skins/prism/pips.webp', './skins/marble/face.webp', './skins/marble/pips.webp',
  './skins/ice/face.webp', './skins/ice/pips.webp', './skins/walnut/face.webp', './skins/walnut/pips.webp',
];

self.addEventListener('install', e => {
  e.waitUntil(caches.open(CACHE).then(c => c.addAll(ASSETS.map(url => new Request(url, { cache: 'reload' })))).then(() => self.skipWaiting()));
});

self.addEventListener('activate', e => {
  e.waitUntil(
    caches.keys()
      .then(keys => Promise.all(keys.filter(k => k !== CACHE).map(k => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

// Network first, cache as offline fallback.
self.addEventListener('fetch', e => {
  if (e.request.method !== 'GET' || !e.request.url.startsWith(self.location.origin)) return;
  // Revalidate the HTTP cache too: a new app must not mix with old modules.
  e.respondWith(
    fetch(e.request, { cache: 'no-cache' }).then(res => {
      if (res.ok) { const copy = res.clone(); caches.open(CACHE).then(c => c.put(e.request, copy)); }
      return res;
    }).catch(() => caches.match(e.request, { ignoreSearch: true }))
  );
});
