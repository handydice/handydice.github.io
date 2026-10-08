import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LANGUAGES, chooseLanguage, diceName, dieType, translations } from './i18n.js';

assert.equal(chooseLanguage(['de-DE', 'en-US']), 'de');
assert.equal(chooseLanguage(['de-CH']), 'de');
assert.equal(chooseLanguage(['en-GB', 'de']), 'en');
assert.equal(chooseLanguage(['sv-SE', 'de-AT']), 'de', 'unsupported languages are skipped');
assert.equal(chooseLanguage(['fr', 'en', 'de']), 'fr');
assert.equal(chooseLanguage(['DE-at']), 'de');
assert.equal(chooseLanguage(['sv-SE']), 'en');
assert.equal(chooseLanguage([]), 'en');
assert.equal(chooseLanguage(['en-US'], 'de'), 'de', 'manual German overrides the browser');
assert.equal(chooseLanguage(['de-DE'], 'en'), 'en', 'manual English overrides the browser');
assert.equal(chooseLanguage(['de-DE'], 'fr'), 'fr', 'manual French overrides the browser');
assert.equal(chooseLanguage(['de-DE'], 'auto'), 'de');
assert.equal(chooseLanguage(['fr-CA', 'de']), 'fr');
assert.equal(chooseLanguage(['ja', 'fr-BE']), 'fr');
assert.equal(chooseLanguage(['pt-PT']), 'pt', 'Portugal gets the Brazilian table');
assert.equal(chooseLanguage(['en-US'], 'invalid'), 'en');

assert.equal(diceName([6, 6, 20, 6]), `3×${dieType(6)} · ${dieType(20)}`);
assert.equal(diceName([100]), dieType(100));
assert.equal(diceName([]), '');

// Every English UI text has a translation in every language; otherwise users see English fragments.
const read = file => readFileSync(new URL(file, import.meta.url), 'utf8');
const html = read('index.html');
const body = html.slice(html.indexOf('<body>'));
const decode = s => s.replaceAll('&amp;', '&');
const texts = [
  ...[...body.matchAll(/>([^<>]+)</g)].map(m => decode(m[1].trim())).filter(s => /\p{L}/u.test(s)),
  ...[...body.matchAll(/aria-label="([^"]+)"/g)].map(m => decode(m[1])),
  ...[...html.matchAll(/<meta name="description" content="([^"]+)"/g)].map(m => decode(m[1])),
  ...['app.js', 'i18n.js'].flatMap(file => [...read(file).matchAll(/\bt\((['"])(.+?)\1\)/g)].map(m => m[2])),
];
const ownLanguage = new Set(['English', 'Deutsch', 'Français', 'Español', 'Português', 'Italiano', 'Nederlands', 'Polski']); // language names stay in their own language
for (const lang of LANGUAGES.filter(lang => lang !== 'en')) {
  const missing = [...new Set(texts)].filter(text => !ownLanguage.has(text) && !Object.hasOwn(translations[lang], text));
  assert.deepEqual(missing, [], `missing ${lang} translations`);
}

console.log('i18n ok');
