import { t } from './i18n.js';

const win = window, doc = win.document;
const { navigator, localStorage } = win;
const $ = selector => doc.querySelector(selector);
const SEEN_KEY = 'handydice-install-seen', INSTALLED_KEY = 'handydice-installed';
const standalone = win.matchMedia('(display-mode: standalone)');
const ios = /iPhone|iPad|iPod/.test(navigator.userAgent)
  || (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1);
let installed = standalone.matches || navigator.standalone === true || localStorage.getItem(INSTALLED_KEY) === '1';
let promote = localStorage.getItem(SEEN_KEY) !== '1', checked = false, installEvt = null;

function update() {
  $('#install-settings').hidden = installed || !checked;
  $('#install').hidden = installed || (!installEvt && !ios);
  $('#install').textContent = t(ios && !installEvt ? 'Add to Home Screen' : 'Install app');
  $('#install-manual').hidden = installed || !!installEvt || ios;
  if (installed) $('#install-ios').hidden = true;
  $('#install-notice').hidden = installed || !checked || !promote || (!installEvt && !ios);
  if (!$('#install-notice').hidden) localStorage.setItem(SEEN_KEY, '1');
}

function rememberInstalled() {
  installed = true;
  installEvt = null;
  promote = false;
  localStorage.setItem(INSTALLED_KEY, '1');
  update();
}

function dismiss() {
  promote = false;
  localStorage.setItem(SEEN_KEY, '1');
  update();
}

async function install() {
  dismiss();
  if (installed) return;
  if (!installEvt) {
    if (ios) {
      const settings = $('#settings');
      if (!settings.open) $('#open').click(); // Reuse settings' gesture/selection cancellation.
      $('#install-ios').hidden = false;
      $('#install-ios').focus();
    }
    return;
  }
  const event = installEvt;
  installEvt = null; // A browser offer is single-use, including when dismissed.
  update();
  try {
    await event.prompt();
    if ((await event.userChoice).outcome === 'accepted') rememberInstalled();
  } catch (error) {
    // The settings retain the browser-menu instructions if the native offer expired.
    console.warn('Installation prompt failed', error);
  }
}

$('#install').onclick = $('#install-shortcut').onclick = install;
$('#install-dismiss').onclick = () => { dismiss(); $('#open').focus(); };
win.addEventListener('beforeinstallprompt', event => {
  event.preventDefault();
  if (!installed) installEvt = event;
  update();
});
win.addEventListener('appinstalled', rememberInstalled);
standalone.addEventListener('change', () => { if (standalone.matches) rememberInstalled(); });
win.addEventListener('storage', event => {
  if (event.key === INSTALLED_KEY && event.newValue === '1') rememberInstalled();
  if (event.key === SEEN_KEY && event.newValue === '1') { promote = false; update(); }
});

if (installed) rememberInstalled();
else (async () => {
  try {
    if ((await navigator.getInstalledRelatedApps?.())?.some(app => app.platform === 'webapp')) rememberInstalled();
  } catch {
    // Detection is optional; browser install offers and standalone detection still work.
  } finally {
    checked = true;
    update();
  }
})();
