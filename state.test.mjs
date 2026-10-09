import assert from 'node:assert/strict';
import { MAX_DICE } from './dice.js';
import { HISTORY_LIMIT, loadState } from './state.js';

const SURFACES = ['', 'oak'];
const load = saved => loadState(JSON.stringify(saved), SURFACES);

// Nothing stored, broken JSON or too many dice: a fresh round of five D6 with default settings.
for (const json of [null, '{', JSON.stringify({ dice: Array(MAX_DICE + 1).fill({ sides: 6, value: 1 }) })]) {
  const { state, restored } = loadState(json, SURFACES);
  assert.equal(restored, false);
  assert.equal(state.dice.length, 5);
  for (const d of state.dice) assert(d.sides === 6 && d.value >= 1 && d.value <= 6);
}

// A valid round survives unchanged, including 2D table slots and the 3D display choice.
const round = {
  dice: [{ sides: 20, value: 17, tray: 't1', color: 'red', slot: 2 }, { sides: 6, value: 3, slot: 0 }],
  history: ['17 · 3 = 20'],
  sets: [{ name: 'Mine', dice: [6, 20], colors: ['blue', undefined] }],
  lastTray: 't1', surface: 'oak', sound: false, clickMode: 'select', rerolls: 2, view: '3d', tilt: true,
};
const restoredRound = load(round);
assert.equal(restoredRound.restored, true);
assert.deepEqual(restoredRound.state, round);

// Installs from before the display choice, or a corrupted choice, fall back to 2D without losing the round.
for (const view of [undefined, '3D', 'webgl', 1]) {
  const { state, restored } = load({ ...round, view });
  assert.equal(restored, true);
  assert.deepEqual(state, { ...round, view: '2d' });
}

// Only an explicit boolean opt-in may enable sensor tilt in a saved round.
for (const tilt of [true, false, undefined, 'true', 1, null]) {
  assert.deepEqual(load({ ...round, tilt }).state, { ...round, tilt: tilt === true });
}

// Numeric faces are an optional D6 presentation; plain D6 and every other die stay unchanged.
const { state: display } = load({
  dice: [
    { sides: 6, value: 1, numbered: true },
    { sides: 6, value: 2, numbered: false },
    { sides: 20, value: 3, numbered: true },
  ],
  sets: [{ name: 'Numbered', dice: [6, 20], numbered: [true, true] }],
});
assert.deepEqual(display.dice, [
  { sides: 6, value: 1, numbered: true },
  { sides: 6, value: 2 },
  { sides: 20, value: 3 },
]);
assert.deepEqual(display.sets, [{
  name: 'Numbered', dice: [6, 20], colors: [undefined, undefined], numbered: [true, undefined],
}]);

// Broken dice are repaired or dropped, so rendering cannot crash on stored data.
const { state: repaired } = load({
  dice: [
    { sides: 6, value: 9 }, // impossible face → rerolled
    { sides: 0, value: 1 }, // impossible die → dropped
    { sides: 6, value: 2, color: '"><img>', tray: 'x', slot: -1 }, // unknown color, tray and slot → removed
    null,
  ],
});
assert.equal(repaired.dice.length, 2);
assert(repaired.dice[0].value >= 1 && repaired.dice[0].value <= 6);
assert.deepEqual(repaired.dice[1], { sides: 6, value: 2 });

// Unknown fields disappear.
const { state: cleaned } = load({ dice: [], mod: 2, lastTray: 't2' });
assert(!('mod' in cleaned));
assert.equal(cleaned.lastTray, undefined);

// Settings fall back to defaults; history and sets keep only usable entries.
const { state: settings } = load({
  dice: [],
  surface: 'marble', clickMode: 'drag',
  history: ['a', 5, ...Array(HISTORY_LIMIT).fill('b')],
  sets: [{ name: 'ok', dice: [6], colors: ['nope'] }, { name: 'empty', dice: [] }, { name: 'bad', dice: [1] }, { dice: [6] }],
});
assert.deepEqual(settings.dice, []);
assert.equal(settings.surface, '');
assert.equal(settings.clickMode, 'direct');
assert.equal(settings.history.length, HISTORY_LIMIT);
assert.equal(settings.history[0], 'a');
assert.deepEqual(settings.sets, [{ name: 'ok', dice: [6], colors: [undefined] }]);

console.log('state ok');
