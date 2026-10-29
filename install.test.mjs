import assert from 'node:assert/strict';

const seen = 'handydice-install-seen', installed = 'handydice-installed';
let session = 0;
async function open({ userAgent = 'Android', standalone = false, iosStandalone = false, storage = new Map(), related } = {}) {
  const win = new EventTarget(), media = new EventTarget();
  media.matches = standalone;
  const elements = new Map(['install-settings', 'install', 'install-manual', 'install-ios', 'install-notice',
    'install-shortcut', 'install-dismiss', 'settings', 'open'].map(id => [id, {
    hidden: true, open: false,
    showModal() { this.open = true; },
    focus() { win.document.activeElement = this; },
  }]));
  elements.get('open').click = () => elements.get('settings').showModal();
  win.document = { querySelector: selector => elements.get(selector.slice(1)) };
  win.navigator = { userAgent, maxTouchPoints: 5, standalone: iosStandalone, getInstalledRelatedApps: related };
  win.localStorage = { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) };
  win.matchMedia = () => media;
  globalThis.window = win;
  await import(`./install.js?session=${session++}`);
  await new Promise(setImmediate);
  return { win, media, storage, el: id => elements.get(id) };
}
function offer(win, outcome = 'dismissed') {
  const event = new Event('beforeinstallprompt', { cancelable: true });
  let calls = 0;
  event.prompt = async () => { calls++; };
  event.userChoice = Promise.resolve({ outcome });
  win.dispatchEvent(event);
  assert(event.defaultPrevented, 'the browser must not interrupt play with its own promotion');
  return () => calls;
}

try {
  // iPhone and desktop-mode iPad offer instructions only on demand; showing the hint is once per browser.
  for (const userAgent of ['iPhone', 'Macintosh']) {
    const app = await open({ userAgent });
    assert.equal(app.el('install-notice').hidden, false);
    assert.equal(app.el('settings').open, false, 'opening the site must never open a dialog');
    assert.equal(app.el('install-ios').hidden, true);
    assert.equal(app.storage.get(seen), '1');
    await app.el('install-shortcut').onclick();
    assert.equal(app.el('settings').open, true);
    assert.equal(app.el('install-ios').hidden, false);
    assert.equal(app.win.document.activeElement, app.el('install-ios'));
    assert.equal(app.el('install-notice').hidden, true);
    const next = await open({ userAgent, storage: app.storage });
    assert.equal(next.el('install-notice').hidden, true, 'a reload must not repeat the hint');
    assert.equal(next.el('install-settings').hidden, false, 'manual installation remains available');
    assert.equal(next.el('install').hidden, false);
  }

  // Android offers are deferred until a tap, consumed once, and do not re-promote after dismissal.
  {
    const app = await open();
    assert.equal(app.el('install-notice').hidden, true);
    assert.equal(app.storage.has(seen), false, 'an unavailable offer must not consume the one-time hint');
    const calls = offer(app.win);
    assert.equal(calls(), 0);
    assert.equal(app.el('install-notice').hidden, false);
    app.el('install-dismiss').onclick();
    assert.equal(app.el('install-notice').hidden, true);
    assert.equal(app.el('install').hidden, false, 'dismissing the hint must not discard the install offer');
    await app.el('install').onclick();
    assert.equal(calls(), 1);
    assert.equal(app.el('install').hidden, true);
    assert.equal(app.el('install-manual').hidden, false);
    const second = offer(app.win);
    assert.equal(app.el('install-notice').hidden, true);
    await app.el('install').onclick();
    assert.equal(second(), 1, 'a fresh browser offer is still usable from settings');
    const next = await open({ storage: app.storage });
    offer(next.win);
    assert.equal(next.el('install-notice').hidden, true);
  }

  // Both standalone signals and remembered installation suppress every installation surface, even on reload.
  for (const options of [{ standalone: true }, { iosStandalone: true }, { storage: new Map([[installed, '1']]) }]) {
    const app = await open({ userAgent: 'iPhone', ...options });
    offer(app.win);
    assert.equal(app.el('install-notice').hidden, true);
    assert.equal(app.el('install-settings').hidden, true);
    assert.equal(app.storage.get(installed), '1');
    const browser = await open({ storage: app.storage });
    offer(browser.win);
    assert.equal(browser.el('install-settings').hidden, true);
  }

  // A delayed Android installation query must finish before an offer can flash on screen.
  {
    let resolve;
    const app = await open({ related: () => new Promise(done => { resolve = done; }) });
    offer(app.win);
    assert.equal(app.el('install-notice').hidden, true);
    assert.equal(app.storage.has(seen), false);
    resolve([{ platform: 'webapp' }]);
    await new Promise(setImmediate);
    assert.equal(app.el('install-settings').hidden, true);
    assert.equal(app.storage.get(installed), '1');
  }

  // An unsupported/rejected optional query must not break normal installation.
  {
    const app = await open({ related: async () => { throw new Error('unavailable'); } });
    offer(app.win, 'accepted');
    assert.equal(app.el('install-notice').hidden, false);
    await app.el('install').onclick();
    assert.equal(app.el('install-settings').hidden, true);
    assert.equal(app.storage.get(installed), '1');
  }

  // Installation via browser UI, a display-mode transition, or another tab hides a visible hint immediately.
  for (const signal of ['appinstalled', 'display-mode', 'storage']) {
    const app = await open();
    offer(app.win);
    if (signal === 'display-mode') { app.media.matches = true; app.media.dispatchEvent(new Event('change')); }
    else if (signal === 'storage') {
      const event = new Event('storage');
      event.key = installed; event.newValue = '1';
      app.win.dispatchEvent(event);
    } else app.win.dispatchEvent(new Event(signal));
    assert.equal(app.el('install-notice').hidden, true);
    assert.equal(app.el('install-settings').hidden, true);
  }
} finally {
  delete globalThis.window;
}
console.log('installation state ok');
