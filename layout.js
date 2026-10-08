// Largest square dice that fit the whole table, including reserved empty slots.
export function fitDice(width, height, count, gap, maxSize) {
  let columns = 1, size = 0;
  for (let c = 1; c <= count; c++) {
    const rows = Math.ceil(count / c);
    const candidate = Math.min(maxSize, (width - (c - 1) * gap) / c, (height - (rows - 1) * gap) / rows);
    if (candidate > size || (candidate === size && rows < Math.ceil(count / columns))) { columns = c; size = candidate; }
  }
  return { columns, size };
}
