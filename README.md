# HandyDice

**Roll dice on your phone. No ads, no tracking, no sign-up, no catch.**

👉 **[handydice.github.io](https://handydice.github.io)**: open it, tap Roll, done. Add it to your home screen and it
works like an app, offline too.

<p align="center">
  <img src="screenshots/hero.webp" width="24%" alt="Five white dice on green casino felt, two set aside in the tray">
  <img src="screenshots/rpg.webp" width="24%" alt="Colorful D4 to D100 roleplaying dice on an oak table">
  <img src="screenshots/skins.webp" width="24%" alt="Twelve D6 in painted skins: opal, wood, galaxy, jade, amber and more">
  <img src="screenshots/trays.webp" width="24%" alt="Light theme with dice collected in a top and a bottom tray">
</p>

## Why another dice app?

Search for "dice" in any app store and you get hundreds of results. Most show you a banner ad after every roll, want
a subscription for a second color or ask for permissions a dice roller has no business asking for. HandyDice is
the one we wanted for our own game nights:

- **Free, for real.** No ads, no in-app purchases, no premium tier. MIT licensed.
- **Private.** No accounts, no analytics, no trackers, no cookies. Your dice and settings stay on your device.
- **Offline.** Once loaded, it works without a connection, at the cabin or on the train.
- **Fair.** Every roll comes from the browser's cryptographic random generator, with rejection sampling so no
  face is even slightly favored.
- **Nothing to install.** It's a web app: one link for iPhone, Android and desktop. Installing to the home screen
  is optional and takes two taps.
- **Small.** About 1,200 lines of plain JavaScript, no framework, no build step. Read it in an evening.

## What it does

- **Up to 12 dice:** D4, D6 (pips or numbers), D8, D10, D12, D20 and D100.
- **Two trays** to set dice aside between rolls, like Yahtzee or Farkle at the table. Only the dice left on the
  table are rerolled, and every tray shows its own total.
- **Ready-made sets** for Yahtzee, That's Pretty Clever, Clever³, roleplaying (D4–D20) and percentile dice.
  Save your own with one tap.
- **12 painted skins** (opal, galaxy, jade, amber, steel, ice, …) plus 11 plain colors, per die.
- **5 table surfaces:** casino felt, oak, crafts, blue wood, terracotta. Light, dark or automatic theme.
- **Feels like real dice:** a dice cup animation with sound, or spinning dice. Shake your phone to roll.
- **Accessible:** full keyboard control, an optional two-tap mode instead of dragging, respects reduced motion.
- **8 languages:** English, German, French, Spanish, Portuguese (Brazil), Italian, Dutch and Polish, following
  your browser or chosen manually.
- **Remembers your round:** dice, trays, sets and the last 30 results survive a reload.

<p align="center">
  <img src="screenshots/add-die.webp" width="24%" alt="Add die screen with die types and the color and skin palette">
  <img src="screenshots/settings.webp" width="24%" alt="Settings with sound, theme and table surface choices">
</p>

## Install

- **iPhone / iPad:** open the link in Safari → Share → *Add to Home Screen*.
- **Android:** open the link in Chrome → *Install app* (in the settings screen or the browser menu).
- **Desktop:** Chrome and Edge show an install icon in the address bar. Or just keep the tab.

## Development

Any static file server works:

```sh
python3 -m http.server 8000
```

Then open http://localhost:8000. The service worker needs `localhost` or HTTPS.

```sh
npm test   # = node --test, runs every *.test.mjs
```

Touch smoke check: tap the Roll button, then hold it for 1–3 seconds and release. Each action must add
exactly one history entry. Enter and Space must still roll; a canceled touch or dropping a die onto
the Roll button must not. The button uses pointer release because mobile long presses can suppress clicks.

| File | Responsibility |
| --- | --- |
| `index.html` | Markup in English (source language), inline theme script to avoid a flash |
| `app.js` | UI wiring: rendering, tap/drag/keyboard input, roll animation, dialogs, settings, shake |
| `dice.js` | Pure dice rules: unbiased rolls, trays, table slots |
| `state.js` | Load and validate saved state; storage keys |
| `i18n.js` | Language choice, translations, DOM localization |
| `layout.js` | Largest dice size that fits the table |
| `sound.js` | Preloaded Web Audio effects |
| `viewport.js` | App height, including the iOS PWA full-screen workaround |
| `sw.js` | Service worker: network first, precached offline fallback |

Pure logic (`dice.js`, `state.js`, `layout.js`, `chooseLanguage`) is tested with `node:assert`; the tests
also check that every UI text is translated into every language and that `sw.js` precaches every asset.

New language: add its code to `LANGUAGES`, a table to `translations` in `i18n.js`, a
`manifest.<code>.webmanifest`, and a button in `#language` (`index.html`).

### Release

1. Bump `CACHE` in `sw.js`; add new files to `ASSETS` (the tests fail if one is missing).
2. Push to `main`: GitHub Actions runs the tests and publishes to GitHub Pages (`.github/workflows/pages.yml`).

## License

[MIT](LICENSE). Contributions and translations welcome.
