import assert from 'node:assert/strict';
import { assignSlots, compactSlots, isSides, putAside, randomFaceValues, roll, rollable, shuffle, SIDES, total } from './dice.js';

for (const sides of [2, 4, 6, 8, 10, 12, 20, 100, 7]) {
  const counts = new Array(sides + 1).fill(0);
  const n = sides * 2000;
  for (let i = 0; i < n; i++) {
    const v = roll(sides);
    assert(Number.isInteger(v) && v >= 1 && v <= sides, `D${sides} -> ${v}`);
    counts[v]++;
  }
  for (let f = 1; f <= sides; f++) {
    assert(Math.abs(counts[f] - 2000) < 300, `D${sides} face ${f}: ${counts[f]}`);
  }
}
assert.equal(total([{ value: 3 }, { value: 6 }]), 9);
assert.equal(total([]), 0);

assert(isSides(4) && isSides(100));
assert(!isSides(7) && !isSides(1000) && !isSides('6'));

const first = { sides: 6, value: 2 };
const rolled = { sides: 6, value: 4 };
const other = { sides: 6, value: 6, tray: 'b1' };
const dice = [first, rolled, other];
putAside(dice, first, 'b1', other);
assert.deepEqual(dice, [rolled, first, other]);
assert.equal(first.tray, 'b1');
putAside(dice, rolled, 'b1');
assert.deepEqual(dice, [first, other, rolled]);
putAside(dice, rolled, 't1', other);
assert.deepEqual(dice, [first, rolled, other]);
assert.equal(rolled.tray, 't1');
assert.equal(rolled.value, 4, 'sorting and moving keeps the rolled value');

// Next roll: table dice only, or everything once all dice are in trays.
const a = { sides: 6, value: 1 }, b = { sides: 6, value: 2, tray: 'b1' };
assert.deepEqual(rollable([a, b]), [a]);
b.tray = undefined;
const allAside = [{ ...a, tray: 't1' }, { ...b, tray: 'b1' }];
assert.equal(rollable(allAside), allAside);

// A die in a tray keeps its table slot free; new dice and returning dice go behind the highest slot.
const kept = { sides: 6, value: 1, slot: 0 };
const aside = { sides: 6, value: 2, slot: 1, tray: 'b1' };
const added = { sides: 6, value: 3 };
const back = { sides: 6, value: 4 };
assignSlots([kept, aside, added, back]);
assert.deepEqual([kept.slot, aside.slot, added.slot, back.slot], [0, 1, 2, 3]);
assignSlots([kept, aside, added, back]);
assert.deepEqual([kept.slot, aside.slot, added.slot, back.slot], [0, 1, 2, 3], 'assigning again is stable');

// A roll assigns slots in the given order (the caller shuffles first) and frees the slots of tray dice.
const all = [kept, aside, added, back];
compactSlots(all, [back, added, kept]);
assert.deepEqual([kept.slot, added.slot, back.slot], [2, 1, 0]);
assert.equal(aside.slot, undefined);

// Shuffling keeps every die exactly once and actually mixes the order.
const dice01234 = [0, 1, 2, 3, 4];
for (let i = 0; i < 100; i++) {
  const before = [...dice01234];
  shuffle(dice01234);
  assert.deepEqual([...dice01234].sort(), [0, 1, 2, 3, 4]);
  if (dice01234.some((d, index) => d !== before[index])) break;
  assert(i < 99, 'shuffle never changed the order');
}

// Every equally likely RNG path of randomFaceValues: an odometer over the injected integer(bound) picks.
function everyNumbering(sides) {
  const maps = [];
  let picks = [];
  for (;;) {
    const bounds = [];
    maps.push(randomFaceValues(sides, bound => picks[bounds.push(bound) - 1] ?? 1));
    let i = bounds.length - 1;
    while (i >= 0 && (picks[i] ?? 1) === bounds[i]) i--;
    if (i < 0) return maps;
    picks = bounds.map((_, j) => j < i ? picks[j] ?? 1 : j === i ? (picks[j] ?? 1) + 1 : 1);
  }
}
const assertNumbering = (sides, map) => {
  assert.equal(map.length, sides);
  assert.deepEqual([...map].sort((x, y) => x - y), Array.from({ length: sides }, (_, i) => i + 1), `D${sides} ${map}`);
  for (let k = 1; k <= sides; k++) assert.equal(map[k - 1] + map[sides - k], sides + 1, `D${sides} opposite of ${k}`);
};
for (const [sides, paths] of [[4, 2 * 4], [6, 6 * 8]]) {
  const maps = everyNumbering(sides);
  assert.equal(maps.length, paths, `D${sides}: pair permutations x flips`);
  assert.equal(new Set(maps.map(String)).size, paths, `D${sides}: each path is a distinct numbering`);
  for (let k = 1; k <= sides; k++) {
    const counts = new Array(sides + 1).fill(0);
    for (const map of maps) counts[map[k - 1]]++;
    assert.deepEqual(counts.slice(1), new Array(sides).fill(paths / sides), `D${sides} face ${k} is uniform`);
  }
  maps.forEach(map => assertNumbering(sides, map));
}
for (const sides of SIDES) assertNumbering(sides, randomFaceValues(sides));
for (const pick of [() => 1, bound => bound]) {
  const map = randomFaceValues(100, pick);
  assertNumbering(100, map);
  assert.equal(map[0] + map[99], 101);
  assert.equal(map[49] + map[50], 101);
}
assert.throws(() => randomFaceValues(7), RangeError);

console.log('dice ok');
