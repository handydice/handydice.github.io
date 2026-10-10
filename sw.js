// Bump on every release so installed clients precache the new files.
const CACHE = 'handydice-2d-3d-v15';
const ASSETS = [
  './', './index.html', './style.css',
  './app.js', './dice.js', './layout.js', './i18n.js', './sound.js', './state.js', './viewport.js',
  './dice3d.js', './physics3d.js', './renderer3d.js', './scene3d.js',
  './story.js', './icons/story/LICENSE',
  './icons/story/house.svg', './icons/story/lightbulb.svg', './icons/story/sleep.svg', './icons/story/speech.svg', './icons/story/clock.svg', './icons/story/arrow.svg',
  './icons/story/lock.svg', './icons/story/footprint.svg', './icons/story/flame.svg', './icons/story/sheep.svg', './icons/story/magnet.svg', './icons/story/learner.svg',
  './icons/story/message.svg', './icons/story/pyramid.svg', './icons/story/tower.svg', './icons/story/rainbow.svg', './icons/story/tree.svg', './icons/story/eye.svg',
  './icons/story/earth.svg', './icons/story/plane.svg', './icons/story/smile.svg', './icons/story/building.svg', './icons/story/flashlight.svg', './icons/story/apple.svg',
  './icons/story/key.svg', './icons/story/shooting-star.svg', './icons/story/question.svg', './icons/story/tent.svg', './icons/story/spinning-object.svg', './icons/story/frown.svg',
  './icons/story/beetle.svg', './icons/story/die.svg', './icons/story/magnifier.svg', './icons/story/shadow.svg', './icons/story/hand.svg', './icons/story/turtle.svg',
  './icons/story/masks.svg', './icons/story/fish.svg', './icons/story/parachute.svg', './icons/story/keyhole.svg', './icons/story/sunflower.svg', './icons/story/arrows.svg',
  './icons/story/book.svg', './icons/story/bridge.svg', './icons/story/abacus.svg', './icons/story/moon.svg', './icons/story/wand.svg', './icons/story/bee.svg',
  './icons/story/alien.svg', './icons/story/lightning.svg', './icons/story/phone.svg', './icons/story/scales.svg', './icons/story/direction.svg', './icons/story/cane.svg',
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
