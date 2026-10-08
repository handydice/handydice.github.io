import { MAX_DICE, assignSlots, compactSlots, putAside, roll, rollable, setSides, shuffle, total } from './dice.js';
import { LANGUAGE_KEY, diceName, dieType, languagePreference, localize, t } from './i18n.js';
import { fitDice } from './layout.js';
import { HISTORY_LIMIT, STATE_KEY, THEME_KEY, loadState, saveState } from './state.js';
import { loadSounds, playSound, resumeSounds } from './sound.js';
import { watchViewport } from './viewport.js';

localize(document);

const $ = s => document.querySelector(s);
const COLOR_NAMES = {
  white: t('White'), red: t('Red'), blue: t('Blue'), yellow: t('Yellow'), green: t('Green'),
  orange: t('Orange'), brown: t('Brown'), purple: t('Purple'), turquoise: t('Turquoise'), pink: t('Pink'), gray: t('Dark gray'),
  'skin-opal': t('Opal'), 'skin-wood': t('Wood'), 'skin-galaxy': t('Galaxy'), 'skin-jade': t('Jade'), 'skin-amber': t('Amber'), 'skin-terrazzo': t('Terrazzo'),
  'skin-ruby': t('Ruby'), 'skin-steel': t('Steel'), 'skin-prism': t('Prism'), 'skin-marble': t('Black marble'), 'skin-ice': t('Ice'), 'skin-walnut': t('Walnut'),
};
const TYPE_OPTIONS = [
  { sides: 4 }, { sides: 6 }, { sides: 6, numbered: true }, { sides: 8 },
  { sides: 10 }, { sides: 12 }, { sides: 20 }, { sides: 100 },
];
const SETS = [
  { name: `1×${dieType(6)}`, dice: [6] },
  { name: `2×${dieType(6)}`, dice: [6, 6] },
  { name: `${t('Yahtzee')} · 5×${dieType(6)}`, dice: [6, 6, 6, 6, 6] },
  {
    name: `${t("That's Pretty Clever")} · 6×${dieType(6)}`,
    dice: [6, 6, 6, 6, 6, 6],
    colors: ['blue', 'yellow', 'green', 'orange', 'purple', 'white'],
  },
  {
    name: `Clever³ · 6×${dieType(6)}`, // game title, the same in every language
    dice: [6, 6, 6, 6, 6, 6],
    colors: ['white', 'brown', 'blue', 'turquoise', 'pink', 'yellow'],
  },
  { name: dieType(20), dice: [20] },
  { name: `${t('Roleplaying')} · ${dieType(4)}–${dieType(20)}`, dice: [4, 6, 8, 10, 12, 20], numbered: [undefined, true] },
  { name: `${t('Percentile')} · ${dieType(100)}`, dice: [100] },
];
// Pip positions in a 3×3 grid (0 = top left … 8 = bottom right).
const PIPS = [[], [4], [0, 8], [0, 4, 8], [0, 2, 6, 8], [0, 2, 4, 6, 8], [0, 2, 3, 5, 6, 8]];

const tableEl = $('#dice');
const trayEls = [...document.querySelectorAll('.tray')];
// Every drop zone carries its tray in data-tray; '' is the table.
const dropZones = [tableEl, ...trayEls];
const settings = $('#settings'), quick = $('#quickdlg'), editor = $('#die-settings'), creator = $('#add-die');

const { state, restored } = loadState(localStorage.getItem(STATE_KEY), [...$('#surface').children].map(b => b.dataset.surface));
let rolling = false;
let selected = null; // die chosen in two-tap mode
let cancelDrag = null; // ends the running pointer or keyboard drag
let editing = null, adding = null; // dice shown in the edit and add dialogs
let editingSets = false;

for (const tray of trayEls) tray.firstElementChild.dataset.label = tray.dataset.tray === 't1' ? t('Top tray') : t('Bottom tray');
tableEl.dataset.label = t('Table');

// Every state change goes through commit(): slots, DOM and storage stay in sync.
function commit() {
  assignSlots(state.dice);
  render();
  saveState(state);
}

const sound = name => { if (state.sound) playSound(name); };
const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
const dialogOpen = () => !!document.querySelector('dialog[open]');
const elsOf = dice => [...document.querySelectorAll('#dice .die, .tray .die')].filter(el => dice.includes(el.die));
const markPressed = (group, key, value) => {
  for (const b of group.children) b.setAttribute('aria-pressed', b.dataset[key] === value);
};
// Button groups carry their value in a data attribute.
const onPick = (group, key, pick) => {
  group.onclick = e => {
    const value = e.target.closest('button')?.dataset[key];
    if (value !== undefined) pick(value);
  };
};

// Moves a die to a tray ('t1'/'b1', optionally before another die) or back to the table ('').
function moveTo(d, tray, before) {
  if (!tray) {
    delete d.tray;
    return;
  }
  putAside(state.dice, d, tray, before);
  state.lastTray = tray;
}

// Ends a drag or two-tap selection before something else takes over the dice.
function interrupt() {
  cancelDrag?.();
  cancelSelection();
}

function face(d) {
  if (d.sides === 6 && !d.numbered) return `<span class="pips" data-value="${d.value}">${[...Array(9)].map((_, i) => `<i${PIPS[d.value].includes(i) ? ' class="on"' : ''}></i>`).join('')}</span>`;
  return `<span class="num">${d.value}</span><small>${dieType(d.sides)}</small>`;
}
// Die type without a roll: classic D6 shows five pips; numeric dice show their side count.
const typeFace = (sides, numbered = false) => sides === 6 && !numbered ? face({ sides, value: 5 }) : `<span class="num">${sides}</span><small>${dieType(sides)}</small>`;
const sidesFace = (sides, numbered = false) => sides === 6 && !numbered ? typeFace(sides) : `<span class="num">${sides}</span>`;
const shapeOf = sides => `d${sides}`;
const typeKey = d => d.sides === 6 && d.numbered ? '6-number' : String(d.sides);

function dieEl(d) {
  const el = document.createElement('div');
  el.die = d;
  el.className = d === selected ? 'die selected' : 'die';
  el.tabIndex = 0;
  el.setAttribute('role', 'button');
  el.dataset.color = d.color ?? '';
  el.dataset.shape = shapeOf(d.sides);
  const action = state.clickMode === 'edit' ? t('edit') : state.clickMode === 'select' ? t('select') : d.tray ? t('return to table') : t('move to tray');
  const display = d.sides === 6 ? ` ${t(d.numbered ? 'Number' : 'Pips')}` : '';
  el.setAttribute('aria-label', `${d.color ? COLOR_NAMES[d.color] + ' ' : ''}${dieType(d.sides)}${display}: ${d.value}, ${action}`);
  el.setAttribute('aria-describedby', 'drag-help');
  el.setAttribute('aria-pressed', d === selected);
  el.innerHTML = face(d);
  el.onclick = e => {
    e.stopPropagation();
    // Screen readers can click without a pointer sequence; pointer taps are handled in pointerDrag.
    if (e.detail === 0 && !rolling) tapDie(d);
  };
  el.oncontextmenu = e => { e.preventDefault(); openDie(d); };
  keyboardDrag(el, d);
  pointerDrag(el, d);
  return el;
}

function ghost() {
  const el = document.createElement('div');
  el.className = 'die ghost';
  el.setAttribute('aria-hidden', 'true');
  return el;
}

function render() {
  const onTable = state.dice.filter(d => !d.tray);
  tableEl.replaceChildren(...state.dice.filter(d => d.slot !== undefined).sort((a, b) => a.slot - b.slot)
    .map(d => d.tray ? ghost() : dieEl(d)));
  for (const tray of trayEls) {
    const dice = state.dice.filter(d => d.tray === tray.dataset.tray);
    tray.firstElementChild.replaceChildren(...dice.map(dieEl));
    tray.lastElementChild.textContent = dice.length ? `Σ ${total(dice)}` : '';
    tray.lastElementChild.hidden = !dice.length;
    tray.classList.toggle('occupied', dice.length > 0);
  }
  markPressed($('#animation'), 'animation', state.animation);
  markPressed($('#sound'), 'sound', state.sound ? 'on' : 'off');
  markPressed($('#surface'), 'surface', state.surface);
  document.documentElement.dataset.surface = state.surface;
  $('#click-mode').value = state.clickMode;
  // Roll number of the turn: the full roll counts as #1, so the first reroll shows #2.
  $('#total').textContent = onTable.length ? `Σ ${total(onTable)}${state.rerolls ? ` #${state.rerolls + 1}` : ''}` : '';
  $('#roll-label').textContent = state.dice.length && !onTable.length ? t('Reroll all dice') : t('Roll');
  $('#reset').hidden = onTable.length === state.dice.length;
  $('#new-die').disabled = rolling || state.dice.length >= MAX_DICE;
  $('#history').replaceChildren(...state.history.map(h => Object.assign(document.createElement('li'), { textContent: h })));
  fitTable();
}

function fitTable() {
  const style = getComputedStyle(tableEl);
  const width = tableEl.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const height = tableEl.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  const gap = parseFloat(style.gap), landscape = matchMedia('(orientation: landscape)').matches;
  const maxSize = landscape ? 128 : (width - 2 * gap) / 3;
  const { columns, size } = fitDice(width, height, tableEl.children.length, gap, maxSize);
  tableEl.style.setProperty('--dice-columns', columns);
  tableEl.style.setProperty('--dice-size', `${size}px`);
}

// Selection (two-tap mode) and drop zones

function cancelSelection() {
  selected = null;
  document.body.classList.remove('choosing');
  for (const el of document.querySelectorAll('main .selected')) {
    el.classList.remove('selected');
    el.setAttribute('aria-pressed', 'false');
  }
}

function selectDie(d) {
  selected = d;
  document.body.classList.add('choosing');
  for (const el of elsOf(state.dice)) {
    el.classList.toggle('selected', el.die === d);
    el.setAttribute('aria-pressed', el.die === d);
  }
}

function dropSelected(tray, before) {
  const d = selected;
  if (!d) return;
  cancelSelection();
  moveTo(d, tray, before);
  commit();
}

function tapDie(d) {
  if (state.clickMode === 'edit') {
    openDie(d);
    return;
  }
  sound('click');
  if (state.clickMode === 'direct') {
    moveTo(d, d.tray ? '' : state.lastTray ?? 'b1');
    commit();
  } else if (selected === d) cancelSelection();
  else if (selected && d.tray) dropSelected(d.tray, d);
  else selectDie(d);
}

for (const zone of dropZones) zone.onclick = () => { if (!rolling) dropSelected(zone.dataset.tray); };
addEventListener('keydown', e => { if (e.key === 'Escape') cancelSelection(); });
addEventListener('blur', interrupt);

const showDropTarget = target => {
  for (const zone of dropZones) zone.classList.toggle('over', zone === target);
};
function clearDropHints() {
  showDropTarget(null);
  marker.remove();
  document.body.classList.remove('dragging');
}

// Keyboard drag: Enter/Space picks a die up, arrows choose the target, Enter/Space drops, Esc cancels.
const arrowTargets = new Map([['ArrowUp', trayEls[0]], ['ArrowDown', trayEls[1]], ['ArrowLeft', tableEl]]);
function keyboardDrag(el, d) {
  let grabbed = false, target;
  el.onkeydown = e => {
    const confirm = e.key === 'Enter' || e.key === ' ';
    if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10') || (!grabbed && confirm && state.clickMode === 'edit')) {
      e.preventDefault();
      openDie(d);
      return;
    }
    if (rolling || (cancelDrag && !grabbed)) return;
    if (grabbed ? !confirm && e.key !== 'Escape' && !arrowTargets.has(e.key) : !confirm) return;
    e.preventDefault();
    if (!grabbed) {
      cancelSelection();
      grabbed = true;
      el.classList.add('held');
      el.setAttribute('aria-pressed', 'true');
      document.body.classList.add('dragging');
      cancelDrag = () => {
        grabbed = false;
        target = undefined;
        el.classList.remove('held');
        el.setAttribute('aria-pressed', 'false');
        clearDropHints();
        cancelDrag = null;
      };
    } else if (e.key === 'Escape') {
      cancelDrag();
    } else if (confirm) {
      const to = target;
      cancelDrag();
      if (to) moveTo(d, to.dataset.tray);
      commit();
      elsOf([d])[0]?.focus();
    } else {
      target = arrowTargets.get(e.key);
      showDropTarget(target);
    }
  };
  el.onblur = () => { if (grabbed) cancelDrag(); };
}

const inRect = (x, y, el) => {
  const r = el.getBoundingClientRect();
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
};
const dropTargetAt = (x, y) => dropZones.find(zone => inRect(x, y, zone));

// Insertion gap while dragging over a tray: shows where the die will land.
const marker = Object.assign(document.createElement('div'), { className: 'ins' });

// Tray order is the order in state.dice. Target: before the first tray die whose left half (or an
// earlier row) the pointer is on; otherwise at the end.
const trayTarget = (tray, d, x, y) => [...tray.firstElementChild.children].find(c => {
  if (!c.die || c.die === d) return false;
  const r = c.getBoundingClientRect();
  return y < r.top || (y <= r.bottom && x < r.left + r.width / 2);
});

// Tap, long press and drag share the pointer. A tap with a slight swipe stays a tap: dragging starts
// beyond DRAG_SLOP, and within the first DRAG_DELAY_MS only beyond DRAG_FAST_SLOP (a deliberate flick).
const DRAG_SLOP = 12, DRAG_FAST_SLOP = 30, DRAG_DELAY_MS = 150;
function pointerDrag(el, d) {
  el.addEventListener('pointerdown', start => {
    if (cancelDrag || rolling || start.button > 0) return;
    start.preventDefault();
    const r = el.getBoundingClientRect();
    // Over a tray the die shrinks to tray size so the gap next to it stays visible.
    const small = parseFloat(getComputedStyle($('main')).getPropertyValue('--tray-die-size') || '3.6rem') * parseFloat(getComputedStyle(document.documentElement).fontSize) / r.width;
    el.classList.add('held'); // lift as soon as the finger is on it
    let moved = false;
    const holdTimer = setTimeout(() => { cancelDrag?.(); openDie(d); }, 550);
    const move = ev => {
      if (ev.pointerId !== start.pointerId) return;
      const dx = ev.clientX - start.clientX, dy = ev.clientY - start.clientY;
      const dist = Math.hypot(dx, dy);
      if (!moved && (dist < DRAG_SLOP || (ev.timeStamp - start.timeStamp < DRAG_DELAY_MS && dist < DRAG_FAST_SLOP))) return;
      clearTimeout(holdTimer);
      if (!moved) {
        moved = true;
        cancelSelection();
        Object.assign(el.style, {
          position: 'fixed', left: r.left + 'px', top: r.top + 'px', width: r.width + 'px',
          zIndex: 10, pointerEvents: 'none',
        });
        el.classList.add('drag');
        document.body.classList.add('dragging');
      }
      const target = dropTargetAt(ev.clientX, ev.clientY);
      const tray = target === tableEl ? undefined : target;
      el.style.transform = `translate(${dx}px, ${dy}px) scale(${tray ? small : 1.08})`;
      showDropTarget(target);
      // Compute the target without the gap (as on release), otherwise the gap pushes its own target away.
      marker.remove();
      if (tray) tray.firstElementChild.insertBefore(marker, trayTarget(tray, d, ev.clientX, ev.clientY) ?? null);
    };
    const up = ev => {
      if (ev.pointerId !== start.pointerId) return;
      clearTimeout(holdTimer);
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
      removeEventListener('pointercancel', up);
      cancelDrag = null;
      clearDropHints();
      el.classList.remove('held');
      if (ev.type === 'pointerup') {
        if (!moved) {
          tapDie(d);
          return;
        }
        const target = dropTargetAt(ev.clientX, ev.clientY);
        if (target) moveTo(d, target.dataset.tray, target === tableEl ? undefined : trayTarget(target, d, ev.clientX, ev.clientY)?.die);
      }
      if (moved) commit();
    };
    cancelDrag = () => up({ pointerId: start.pointerId, type: 'pointercancel' });
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
    addEventListener('pointercancel', up);
  });
}

// Rolling. Cup animation: collect → button shakes → throw out. Values are fixed only on the throw.
const play = (el, frames, opts) => el.animate(frames, opts).finished.catch(() => {}); // cancelled = finished
const toButton = el => {
  const a = el.getBoundingClientRect(), b = $('#roll').getBoundingClientRect();
  return `translate(${b.x + b.width / 2 - a.x - a.width / 2}px, ${b.y + b.height / 2 - a.y - a.height / 2}px)`;
};
const tumble = () => `rotate(${Math.round((Math.random() - .5) * 720)}deg)`;

// shaken: the phone was shaken – the dice already rattled on the table, so no cup button.
async function doRoll(shaken = false) {
  if (rolling || !state.dice.length || dialogOpen() || (shaken && selected)) return;
  interrupt();
  rolling = true;
  $('#roll').disabled = true;
  $('#new-die').disabled = true;
  document.body.classList.add('rolling', 'sum-pending');
  // Everything in trays → all dice back to the table for a new round.
  const newRound = state.dice.every(d => d.tray);
  const active = rollable(state.dice);
  const calm = reducedMotion();
  const classic = state.animation === 'classic' && !calm;
  document.body.classList.toggle('classic', classic);
  const cup = !calm && !shaken && !classic;
  // Cup sound only with the cup animation; when shaken, the rattle already played.
  if (cup) sound('throw');
  else if (!shaken) sound('shuffle'); // spin and reduced motion rattle on the roll

  if (cup) {
    // 1. Collect: the dice fly into the roll button.
    const els = elsOf(active);
    await Promise.all(els.map((el, i) => play(el,
      [{ transform: 'none', opacity: 1 }, { transform: `${toButton(el)} scale(.25) ${tumble()}`, opacity: 0 }],
      { duration: 200, delay: i * 20, easing: 'ease-in', fill: 'forwards' })));
    // 2. Shake: the button jiggles, the phone buzzes along.
    navigator.vibrate?.([15, 50, 15, 50, 15, 50, 15]);
    await play($('#roll'),
      [{ transform: 'translateX(-7px) rotate(-2.5deg) scale(1.03)' }, { transform: 'translateX(7px) rotate(2.5deg) scale(1.03)' }],
      { duration: 85, iterations: 4, direction: 'alternate', easing: 'ease-in-out' });
  }

  if (newRound) state.dice.forEach(d => { delete d.tray; });
  // Dice left in a tray → reroll since the last roll with all dice; how they got back doesn't matter.
  state.rerolls = state.dice.some(d => d.tray) ? state.rerolls + 1 : 0;
  // Shuffled dice scatter over the table instead of each die returning to its old spot.
  compactSlots(state.dice, shuffle(active));
  // Spin animation: seven quick value changes while the dice wobble on the table.
  if (classic) {
    for (let i = 0; i < 7; i++) {
      active.forEach(d => { d.value = roll(d.sides); });
      render();
      await new Promise(resolve => setTimeout(resolve, 70));
    }
  }
  active.forEach(d => { d.value = roll(d.sides); });
  render();
  document.body.classList.remove('sum-pending');

  // 3. Throw out: one after another from the button to their slots, slightly twisted, with a bounce.
  //    After shaking they hop up in place and land instead. No filter keyframes: WebKit (iOS) renders
  //    an animated filter blurry with an oversized shadow and pops to the sharp die when it ends.
  const out = [...tableEl.children].filter(el => active.includes(el.die));
  if (!classic) await Promise.all(out.map((el, i) => calm
    ? play(el, [{ opacity: 0 }, { opacity: 1 }], { duration: 150 })
    : play(el, [
      { transform: `${shaken ? 'translateY(-2.5rem) scale(1.25)' : `${toButton(el)} scale(.3)`} ${tumble()}`, opacity: 0 },
      { transform: `translate(0, 0) scale(1.1) rotate(${Math.round((Math.random() - .5) * 16)}deg)`, opacity: 1, offset: .7 },
      { transform: 'none', opacity: 1 },
    ], { duration: 420, delay: i * 70, easing: 'cubic-bezier(.2, .8, .3, 1)', fill: 'backwards' })));

  navigator.vibrate?.(25);
  rolling = false;
  $('#roll').disabled = false;
  document.body.classList.remove('rolling', 'classic');
  state.history = [`${state.dice.map(d => d.value).join(' · ')} = ${total(state.dice)}`, ...state.history].slice(0, HISTORY_LIMIT);
  commit();
}

// Roll on release, not only on click: after a long press the browser fires contextmenu and drops the
// click. Only a press that started on the button counts, so dropping a dragged die here does not roll.
// click stays for keyboard and screen readers; doRoll synchronously blocks duplicate activation.
let rollPress = null;
$('#roll').onpointerdown = e => {
  if (!e.isPrimary || e.button !== 0 || e.currentTarget.disabled) return;
  rollPress = e.pointerId;
  e.currentTarget.setPointerCapture(e.pointerId);
};
$('#roll').onpointerup = e => {
  if (e.pointerId !== rollPress) return;
  rollPress = null;
  doRoll();
};
$('#roll').onlostpointercapture = $('#roll').onpointercancel = () => { rollPress = null; };
$('#roll').onclick = () => doRoll();
$('#roll').oncontextmenu = e => e.preventDefault();
$('#reset').onclick = () => {
  if (rolling) return;
  interrupt();
  state.dice.forEach(d => { delete d.tray; });
  commit();
};

// Dialogs

editor.onclick = e => { if (e.target === editor) editor.close(); };
$('#open').onclick = () => {
  interrupt();
  settings.showModal();
};
const currentName = () => diceName(state.dice.map(d => d.sides));
$('#quick').onclick = () => {
  interrupt();
  editingSets = false;
  renderSets();
  $('#set-name').value = currentName();
  $('#save-set button').disabled = !state.dice.length;
  quick.showModal();
};

$('#new-die').onclick = () => {
  if (rolling || state.dice.length >= MAX_DICE) return;
  interrupt();
  adding = { sides: 6, color: 'white', value: roll(6) };
  updatePicker(creator, adding);
  creator.showModal();
  creator.scrollTop = 0;
};
$('#add-form').onsubmit = e => {
  e.preventDefault();
  if (state.dice.length >= MAX_DICE || !e.currentTarget.reportValidity()) return;
  state.dice.push(adding);
  creator.close();
  commit();
};
// Native close events can arrive after the dialog was opened again.
creator.onclose = () => {
  if (creator.open) return;
  adding = null;
  ($('#new-die').disabled ? $('#open') : $('#new-die')).focus();
};

function openDie(d) {
  if (rolling) return;
  sound('click');
  interrupt();
  editing = d;
  updateEditor();
  if (!editor.open) editor.showModal();
  editor.scrollTop = 0;
}

function updateEditor() {
  $('#die-title').textContent = `${t('Edit die')} · ${dieType(editing.sides)}`;
  updatePicker(editor, editing);
  markPressed($('#position'), 'tray', editing.tray ?? '');
  $('#die-order').hidden = !editing.tray;
  const peers = state.dice.filter(d => d.tray === editing.tray);
  const i = peers.indexOf(editing);
  for (const b of $('#order').children) b.disabled = !editing.tray || i + Number(b.dataset.step) < 0 || i + Number(b.dataset.step) >= peers.length;
}

function updatePicker(dialog, d) {
  const type = dialog.querySelector('.die-type');
  const types = dialog.querySelector('.die-types');
  markPressed(dialog.querySelector('.palette'), 'color', d.color || 'white');
  // The pressed type tile doubles as the current die: it wears the chosen color.
  for (const el of dialog.querySelectorAll('.die-types .die')) el.dataset.color = d.color || 'white';
  if (type) type.value = typeKey(d);
  if (types) markPressed(types, 'type', typeKey(d));
}

// The add screen uses eight visual variants; editing keeps the compact native select.
function setupPicker(dialog, getDie, onChange) {
  const type = dialog.querySelector('.die-type'), types = dialog.querySelector('.die-types');
  const chooseType = value => {
    const d = getDie();
    if (!d) return;
    const sides = Number.parseInt(value, 10);
    setSides(d, sides);
    if (value === '6-number') d.numbered = true;
    else if (sides === 6) delete d.numbered;
    onChange();
  };
  if (type) {
    type.innerHTML = TYPE_OPTIONS.map(option => {
      const display = option.sides === 6 ? ` · ${option.numbered ? t('Number') : t('Pips')}` : '';
      return `<option value="${typeKey(option)}">${dieType(option.sides)}${display}</option>`;
    }).join('');
    type.onchange = () => chooseType(type.value);
  } else {
    const buttons = TYPE_OPTIONS.map(option => {
      const b = document.createElement('button');
      b.type = 'button';
      b.dataset.type = typeKey(option);
      const label = option.sides === 6 ? `${dieType(6)} · ${option.numbered ? t('Number') : t('Pips')}` : `${option.sides} ${t('sides')}`;
      b.innerHTML = `<span class="die" data-shape="${shapeOf(option.sides)}" data-color="white" aria-hidden="true">${sidesFace(option.sides, option.numbered)}</span><span>${label}</span>`;
      return b;
    });
    types.replaceChildren(...buttons);
    types.onclick = e => {
      const value = e.target.closest('button')?.dataset.type;
      if (value !== undefined) chooseType(value);
    };
  }
  dialog.querySelector('.palette').replaceChildren(...Object.entries(COLOR_NAMES).map(([color, name]) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'die';
    b.dataset.color = color;
    b.setAttribute('aria-label', name);
    b.title = name;
    b.onclick = () => {
      getDie().color = color;
      onChange();
    };
    return b;
  }));
}
setupPicker(editor, () => editing, () => { commit(); updateEditor(); });
setupPicker(creator, () => adding, () => updatePicker(creator, adding));

onPick($('#position'), 'tray', tray => {
  if (tray === (editing.tray ?? '')) return;
  moveTo(editing, tray);
  commit();
  updateEditor();
});
$('#order').onclick = e => {
  const b = e.target.closest('button');
  if (!b || b.disabled) return;
  const peers = state.dice.filter(d => d.tray === editing.tray);
  const i = peers.indexOf(editing);
  moveTo(editing, editing.tray, peers[i + (Number(b.dataset.step) < 0 ? -1 : 2)]);
  commit();
  updateEditor();
};
$('#remove-die').onclick = () => {
  state.dice.splice(state.dice.indexOf(editing), 1);
  editor.close();
  commit();
};
editor.onclose = () => {
  if (editor.open) return;
  const d = editing;
  editing = null;
  (elsOf([d])[0] ?? $('#new-die')).focus();
};

// Dice sets: own sets on top; the remove button only shows in edit mode.
function presetEl(set) {
  const b = document.createElement('button');
  b.className = 'preset';
  b.innerHTML = `<span class="minis">${set.dice.map((s, i) => `<span class="die" data-shape="${shapeOf(s)}" data-color="${set.colors?.[i] ?? ''}">${typeFace(s, set.numbered?.[i])}</span>`).join('')}</span>`;
  b.append(set.name);
  b.onclick = () => {
    state.dice = set.dice.map((sides, i) => {
      const d = { sides, value: roll(sides), color: set.colors?.[i] || undefined };
      if (sides === 6 && set.numbered?.[i] === true) d.numbered = true;
      return d;
    });
    quick.close();
    commit();
    doRoll();
  };
  return b;
}
function renderSets() {
  const own = state.sets.map(set => {
    const row = document.createElement('div');
    row.className = 'own';
    const del = document.createElement('button');
    del.className = 'remove danger';
    del.innerHTML = '<i class="ico trash" aria-hidden="true"></i>';
    del.setAttribute('aria-label', `${set.name}: ${t('remove')}`);
    del.hidden = !editingSets;
    del.onclick = () => {
      state.sets.splice(state.sets.indexOf(set), 1);
      if (!state.sets.length) editingSets = false;
      saveState(state);
      renderSets();
      (editingSets ? $('#edit-sets') : $('#set-name')).focus();
    };
    row.append(presetEl(set), del);
    return row;
  });
  $('#sets').replaceChildren(...own, ...SETS.map(presetEl));
  $('#edit-sets').hidden = !state.sets.length;
  $('#edit-sets').textContent = editingSets ? t('Done') : t('Edit');
  $('#edit-sets').setAttribute('aria-pressed', editingSets);
}
$('#edit-sets').onclick = () => {
  editingSets = !editingSets;
  renderSets();
};
$('#save-set').onsubmit = e => {
  e.preventDefault();
  if (!state.dice.length) return;
  const numbered = state.dice.map(d => d.sides === 6 && d.numbered === true ? true : undefined);
  const set = { name: $('#set-name').value.trim() || currentName(), dice: state.dice.map(d => d.sides), colors: state.dice.map(d => d.color) };
  if (numbered.some(Boolean)) set.numbered = numbered;
  state.sets.unshift(set);
  saveState(state);
  renderSets();
  $('#set-name').value = currentName();
  quick.scrollTop = 0;
};

// Settings

$('#click-mode').onchange = () => {
  interrupt();
  state.clickMode = $('#click-mode').value;
  commit();
};
function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
  markPressed($('#theme'), 'theme', theme);
  document.querySelector('meta[name=theme-color]').content = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim();
}
onPick($('#theme'), 'theme', setTheme);
setTheme(document.documentElement.dataset.theme);
onPick($('#animation'), 'animation', animation => {
  state.animation = animation;
  commit();
});
onPick($('#surface'), 'surface', surface => {
  state.surface = surface;
  commit();
});
onPick($('#sound'), 'sound', value => {
  state.sound = value === 'on';
  if (state.sound) {
    loadSounds();
    resumeSounds();
  }
  commit();
});
markPressed($('#language'), 'language', languagePreference);
onPick($('#language'), 'language', language => {
  localStorage.setItem(LANGUAGE_KEY, language);
  location.reload(); // rebuild all dynamic texts and install metadata in the same language
});
$('#clearhist').onclick = () => {
  state.history = [];
  commit();
};

// Install: Chrome on Android offers the app by itself; the button brings the offer back on request.
// iOS has no beforeinstallprompt and stays with the share menu.
let installEvt = null;
addEventListener('beforeinstallprompt', e => {
  e.preventDefault();
  installEvt = e;
  $('#install').hidden = false;
});
$('#install').onclick = async () => {
  if (!installEvt) return;
  installEvt.prompt();
  await installEvt.userChoice;
  installEvt = null;
  $('#install').hidden = true;
};
addEventListener('appinstalled', () => { $('#install').hidden = true; });

// Shake to roll: while the phone is shaken the dice rattle on the table; when shaking stops, they are thrown.
// Calibration (m/s², including gravity ≈ 9.8): START = jolt to begin, KEEP = still shaking, STILL_MS = rest before the throw.
const SHAKE_START = 22, SHAKE_KEEP = 15, SHAKE_STILL_MS = 350;
let lastMag = 0, shakeTimer = null;
addEventListener('devicemotion', e => {
  const a = e.accelerationIncludingGravity;
  if (!a || rolling || cancelDrag || selected || dialogOpen() || !state.dice.length) return;
  const m = Math.hypot(a.x ?? 0, a.y ?? 0, a.z ?? 0);
  const jolt = m > SHAKE_START && m - lastMag > 15;
  lastMag = m;
  if (!jolt && !(shakeTimer && m > SHAKE_KEEP)) return;
  if (!shakeTimer) {
    for (const el of elsOf(rollable(state.dice))) el.classList.add('rattle');
    // The rattle starts with the shaking, not with the throw; with reduced motion nothing wobbles.
    if (!reducedMotion()) sound('shuffle');
    navigator.vibrate?.([15, 50, 15, 50, 15]);
  }
  clearTimeout(shakeTimer);
  shakeTimer = setTimeout(() => {
    shakeTimer = null;
    for (const el of document.querySelectorAll('.rattle')) el.classList.remove('rattle');
    doRoll(true);
  }, SHAKE_STILL_MS);
}, { passive: true });
// iOS asks for motion permission only after a user gesture.
addEventListener('click', () => window.DeviceMotionEvent?.requestPermission?.().catch(() => {}), { once: true });

// Startup

if (state.sound) loadSounds();
watchViewport(sizeChanged => {
  if (sizeChanged) {
    interrupt();
    // The layout moved under a running roll: end its animations instead of flying to stale positions.
    if (rolling) for (const animation of document.getAnimations())
      if (animation.effect?.target?.closest('#dice, .tray, #roll')) animation.cancel();
  }
  fitTable();
});
document.addEventListener('visibilitychange', () => { if (document.hidden) interrupt(); });

commit();
document.documentElement.classList.add('ready');
if (!restored) doRoll();
