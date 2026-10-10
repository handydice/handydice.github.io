import assert from 'node:assert/strict';
import { chooseLanguage, diceName, dieType } from './i18n.js';

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


console.log('i18n ok');
