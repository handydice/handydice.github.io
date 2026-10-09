import { COLORS, MAX_DICE, isSides, roll } from './dice.js';

export const STATE_KEY = 'handydice-state';
export const THEME_KEY = 'handydice-theme'; // also read by the inline script in index.html
const DEFAULT_DICE = [6, 6, 6, 6, 6];
const isFace = (value, sides) => Number.isInteger(value) && value >= 1 && value <= sides;
const isTray = tray => tray === 't1' || tray === 'b1';

// Parses a stored round and repairs everything the UI relies on. Missing or broken data starts a
// fresh round (`restored: false`); `surfaces` are the selectable table backgrounds.
export function loadState(json, surfaces) {
  let saved;
  try { saved = JSON.parse(json); } catch {}
  const restored = Array.isArray(saved?.dice) && saved.dice.length <= MAX_DICE;
  const s = restored ? saved : { dice: DEFAULT_DICE.map(sides => ({ sides })) };
  return {
    restored,
    state: {
      dice: s.dice.map(loadDie).filter(Boolean),
      sets: Array.isArray(s.sets) ? s.sets.filter(isSet).map(loadSet) : [],
      lastTray: isTray(s.lastTray) ? s.lastTray : undefined,
      surface: surfaces.includes(s.surface) ? s.surface : '',
      sound: s.sound !== false,
      clickMode: s.clickMode === 'select' ? s.clickMode : 'direct',
      rerolls: Number.isInteger(s.rerolls) && s.rerolls > 0 ? s.rerolls : 0,
      view: s.view === '3d' ? '3d' : '2d',
      tilt: s.tilt === true,
    },
  };
}

function loadDie(d) {
  if (!isSides(d?.sides)) return null;
  const die = { sides: d.sides, value: isFace(d.value, d.sides) ? d.value : roll(d.sides) };
  if (isTray(d.tray)) die.tray = d.tray;
  if (COLORS.includes(d.color)) die.color = d.color;
  if (d.sides === 6 && d.numbered === true) die.numbered = true;
  if (Number.isInteger(d.slot) && d.slot >= 0) die.slot = d.slot;
  return die;
}

function loadSet(set) {
  const numbered = set.dice.map((sides, i) => sides === 6 && set.numbered?.[i] === true ? true : undefined);
  return {
    name: set.name,
    dice: set.dice,
    colors: set.dice.map((_, i) => COLORS.includes(set.colors?.[i]) ? set.colors[i] : undefined),
    ...(numbered.some(Boolean) ? { numbered } : {}),
  };
}

const isSet = set => typeof set?.name === 'string' && Array.isArray(set.dice)
  && set.dice.length > 0 && set.dice.length <= MAX_DICE && set.dice.every(isSides);

export const saveState = state => localStorage.setItem(STATE_KEY, JSON.stringify(state));
