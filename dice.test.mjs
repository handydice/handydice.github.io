import assert from 'node:assert/strict';
import { assignSlots, compactSlots, isSides, putAside, roll, rollable, setSides, shuffle, total } from './dice.js';

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

const edited = { sides: 20, value: 19, tray: 't1', slot: 3, color: 'gray' };
setSides(edited, 20);
assert.deepEqual(edited, { sides: 20, value: 19, tray: 't1', slot: 3, color: 'gray' });
setSides(edited, 6);
const { value, ...rest } = edited;
assert.deepEqual(rest, { sides: 6, tray: 't1', slot: 3, color: 'gray' }, 'a type change keeps tray, color and table slot');
assert(value >= 1 && value <= 6, 'a type change lands the new die with a random face up');
const numbered = { sides: 6, value: 4, numbered: true };
setSides(numbered, 8);
assert(!('numbered' in numbered), 'changing away from D6 removes its presentation');

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

console.log('dice ok');
