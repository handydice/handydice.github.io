export const MAX_DICE = 12;
// skin-* are painted D6 faces from skins/<name>/; on other dice they fall back to a plain color.
export const COLORS = ['white', 'red', 'blue', 'yellow', 'green', 'orange', 'brown', 'purple', 'turquoise', 'pink', 'gray',
  'skin-opal', 'skin-wood', 'skin-galaxy', 'skin-jade', 'skin-amber', 'skin-terrazzo',
  'skin-ruby', 'skin-steel', 'skin-prism', 'skin-marble', 'skin-ice', 'skin-walnut'];

export const SIDES = [4, 6, 8, 10, 12, 20, 100];
export const isSides = n => SIDES.includes(n);

// Unbiased random number 1..sides (rejection sampling avoids modulo bias).
export function roll(sides) {
  const max = 2 ** 32;
  const limit = max - (max % sides);
  const buf = new Uint32Array(1);
  do crypto.getRandomValues(buf); while (buf[0] >= limit);
  return (buf[0] % sides) + 1;
}

// Fresh numbering for a throw: geometric face k (index k-1) shows map[k-1]. Opposite-number pairs
// {v, sides+1-v} are Fisher-Yates shuffled onto geometric pairs {k, sides+1-k} and each flipped by a
// fair coin, so every face is uniform over 1..sides independent of how physics lands the die.
export function randomFaceValues(sides, integer = roll) {
  if (!isSides(sides)) throw new RangeError(`unsupported die D${sides}`);
  const half = sides / 2;
  const pairs = Array.from({ length: half }, (_, i) => i + 1);
  for (let i = half - 1; i > 0; i--) {
    const j = integer(i + 1) - 1;
    [pairs[i], pairs[j]] = [pairs[j], pairs[i]];
  }
  const map = new Uint8Array(sides);
  for (let k = 1; k <= half; k++) {
    const v = integer(2) === 1 ? pairs[k - 1] : sides + 1 - pairs[k - 1];
    map[k - 1] = v;
    map[sides - k] = sides + 1 - v;
  }
  return map;
}

export const total = dice => dice.reduce((s, d) => s + d.value, 0);

// Dice of the next roll: the table dice, or all of them once every die is in a tray.
export const rollable = dice => dice.every(d => d.tray) ? dice : dice.filter(d => !d.tray);

// Without a target, append; otherwise insert right before it. Tray order is the order in `dice`.
export function putAside(dice, d, tray, before) {
  dice.splice(dice.indexOf(d), 1);
  dice.splice(before ? dice.indexOf(before) : dice.length, 0, d);
  d.tray = tray;
}

// A new type is a different die that lands with a random face up; slot, tray and color stay.
export function setSides(d, sides) {
  if (d.sides === sides) return;
  d.sides = sides;
  d.value = roll(sides);
  if (sides !== 6) delete d.numbered;
}

// Table slots (2D): a die moved to a tray keeps its slot free until the next roll, so nothing shifts on
// the table. Dice without a slot (new, or back from a tray after a roll) are appended.
export function assignSlots(dice) {
  let next = Math.max(-1, ...dice.map(d => d.slot ?? -1)) + 1;
  for (const d of dice) if (!d.tray && d.slot === undefined) d.slot = next++;
}

// Fisher–Yates, in place.
export function shuffle(dice) {
  for (let i = dice.length - 1; i > 0; i--) {
    const j = roll(i + 1) - 1;
    [dice[i], dice[j]] = [dice[j], dice[i]];
  }
  return dice;
}

// A roll closes the gaps: rolled dice get slots 0..n-1 in the given order, tray dice lose theirs.
// Callers pass the rolled dice already in table order, e.g. shuffled so dice scatter on the table.
export function compactSlots(dice, rolled) {
  rolled.forEach((d, i) => { d.slot = i; });
  for (const d of dice) if (d.tray) delete d.slot;
}
