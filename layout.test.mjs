import assert from 'node:assert/strict';
import { fitDice } from './layout.js';

for (const [width, height] of [[320, 500], [520, 250], [340, 180], [220, 140]]) {
  for (let count = 1; count <= 12; count++) {
    const { columns, size } = fitDice(width, height, count, 12, 128);
    const rows = Math.ceil(count / columns);
    assert(size > 0 && size <= 128);
    assert(columns * size + (columns - 1) * 12 <= width + 1e-9);
    assert(rows * size + (rows - 1) * 12 <= height + 1e-9);
    for (let c = 1; c <= count; c++) {
      const r = Math.ceil(count / c);
      assert(Math.min(128, (width - (c - 1) * 12) / c, (height - (r - 1) * 12) / r) <= size + 1e-9);
    }
  }
}
assert.deepEqual(fitDice(320, 500, 0, 12, 128), { columns: 1, size: 0 });
assert.equal(fitDice(0, 0, 12, 12, 128).size, 0);
assert.deepEqual(fitDice(320, 500, 5, 16, 96), { columns: 3, size: 96 });
console.log('dice layout fit ok');
