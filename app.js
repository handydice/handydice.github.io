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
const renderError = document.createElement('p');
renderError.id = 'render-error';
renderError.setAttribute('role', 'alert');
renderError.hidden = true;
document.body.append(renderError);
const flickDebug = document.createElement('output');
flickDebug.id = 'flick-debug';
flickDebug.hidden = true;
flickDebug.setAttribute('aria-live', 'polite');
$('main').append(flickDebug);
function showRenderError(error) {
  renderError.textContent = t('3D rendering needs WebGL2. Choose 2D in Settings, or enable graphics acceleration and reload.');
  renderError.hidden = false;
  console.error(error);
}

// 3D view only: scene3d.js and its WebGL context load on demand, so 2D never touches WebGL.
let scene = null, canvas = null, sceneToken = 0;
const contextLost = e => {
  e.preventDefault();
  showRenderError(new Error('WebGL context lost'));
};
const contextRestored = () => { renderError.hidden = true; };
async function startScene() {
  const token = ++sceneToken;
  let DiceScene;
  try { ({ DiceScene } = await import('./scene3d.js')); }
  catch (error) { if (token === sceneToken) showRenderError(error); return; }
  if (token !== sceneToken) return; // switched back to 2D while loading
  canvas = Object.assign(document.createElement('canvas'), { id: 'dice-canvas' });
  canvas.setAttribute('aria-hidden', 'true');
  canvas.addEventListener('webglcontextlost', contextLost);
  canvas.addEventListener('webglcontextrestored', contextRestored);
  document.body.prepend(canvas);
  try { scene = new DiceScene(canvas, tableEl); }
  catch (error) { showRenderError(error); return; }
  scene.onError = showRenderError;
  let lastFlickDie = null;
  if (new URLSearchParams(location.search).has('debug')) scene.onFlickRandomize = (die, triggered) => {
    if (!triggered) lastFlickDie = die;
    if (die !== lastFlickDie) return;
    flickDebug.textContent = `Last flick · D${die.sides} · Crypto: ${triggered ? 'YES' : 'NO'}`;
    flickDebug.dataset.triggered = triggered;
    flickDebug.hidden = false;
  };
  scene.onSettle = () => {
    const focusedDie = document.activeElement?.die;
    commit();
    if (focusedDie) elsOf([focusedDie])[0]?.focus();
  };
  render();
}
function stopScene() {
  sceneToken++;
  canvas?.removeEventListener('webglcontextlost', contextLost);
  canvas?.removeEventListener('webglcontextrestored', contextRestored);
  scene?.dispose();
  // A failed constructor may still have allocated a context without returning a scene.
  if (!scene && canvas) canvas.getContext('webgl2')?.getExtension('WEBGL_lose_context')?.loseContext();
  scene = null;
  canvas?.remove();
  canvas = null;
  renderError.hidden = true;
  flickDebug.hidden = true;
}
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

// Every state change goes through commit(): slots, scene, DOM and storage stay in sync.
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
  const flat = state.view === '2d';
  const el = document.createElement('div');
  el.die = d;
  // 3D: a transparent focus and drop target over the rendered mesh; 2D draws the die itself.
  el.className = `die${flat ? '' : ' rendered-die'}${d === selected ? ' selected' : ''}`;
  el.tabIndex = 0;
  el.setAttribute('role', 'button');
  el.dataset.color = d.color ?? '';
  if (flat) el.dataset.shape = shapeOf(d.sides);
  const action = state.clickMode === 'edit' ? t('edit') : state.clickMode === 'select' ? t('select') : d.tray ? t('return to table') : t('move to tray');
  const display = d.sides === 6 ? ` ${t(d.numbered ? 'Number' : 'Pips')}` : '';
  el.setAttribute('aria-label', `${d.color ? COLOR_NAMES[d.color] + ' ' : ''}${dieType(d.sides)}${display}: ${d.value}, ${action}`);
  el.setAttribute('aria-describedby', flat ? 'drag-help' : 'drag-help rotate-help');
  el.setAttribute('aria-pressed', d === selected);
  if (flat) el.innerHTML = face(d);
  el.onclick = e => {
    e.stopPropagation();
    // Screen readers can click without a pointer sequence; pointer taps are handled in pointerDrag.
    if (e.detail === 0 && !rolling) tapDie(d);
  };
  // 3D opens the editor from the right button in pointerDrag; there it also rotates.
  el.oncontextmenu = e => { e.preventDefault(); if (flat) openDie(d); };
  keyboardDrag(el, d);
  pointerDrag(el);
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
  // 2D keeps tray dice's table slots free (ghosts) so nothing shifts; 3D places dice physically.
  tableEl.replaceChildren(...state.view === '2d'
    ? state.dice.filter(d => d.slot !== undefined).sort((a, b) => a.slot - b.slot).map(d => d.tray ? ghost() : dieEl(d))
    : onTable.map(dieEl));
  for (const tray of trayEls) {
    const dice = state.dice.filter(d => d.tray === tray.dataset.tray);
    tray.firstElementChild.replaceChildren(...dice.map(dieEl));
    tray.lastElementChild.textContent = dice.length ? `Σ ${total(dice)}` : '';
    tray.lastElementChild.hidden = !dice.length;
    tray.classList.toggle('occupied', dice.length > 0);
  }
  markPressed($('#animation'), 'animation', state.animation);
  markPressed($('#view'), 'view', state.view);
  document.documentElement.dataset.view = state.view;
  markPressed($('#sound'), 'sound', state.sound ? 'on' : 'off');
  markPressed($('#tilt'), 'tilt', state.tilt ? 'on' : 'off');
  markPressed($('#surface'), 'surface', state.surface);
  document.documentElement.dataset.surface = state.surface;
  $('#click-mode').value = state.clickMode;
  // Roll number of the turn: the full roll counts as #1, so the first reroll shows #2.
  $('#total').textContent = onTable.length ? `Σ ${total(onTable)}${state.rerolls ? ` #${state.rerolls + 1}` : ''}` : '';
  $('#roll-label').textContent = state.dice.length && !onTable.length ? t('Reroll all dice') : t('Roll');
  $('#reset').hidden = onTable.length === state.dice.length;
  $('#new-die').disabled = rolling || state.dice.length >= MAX_DICE;
  $('#history').replaceChildren(...state.history.map(h => Object.assign(document.createElement('li'), { textContent: h })));
  if (state.view === '2d') fitTable();
  else scene?.sync(state.dice, new Map(elsOf(state.dice).map(el => [el.die, el])), state.surface, document.documentElement.dataset.theme);
}

function fitTable() {
  if (state.view === '3d') {
    scene?.layout();
    return;
  }
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
  scene?.refreshEntries(); scene?.invalidate();
}

function selectDie(d) {
  selected = d;
  document.body.classList.add('choosing');
  for (const el of elsOf(state.dice)) {
    el.classList.toggle('selected', el.die === d);
    el.setAttribute('aria-pressed', el.die === d);
  }
  scene?.refreshEntries(); scene?.invalidate();
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
    if (state.view === '3d' && grabbed && e.shiftKey && e.key.startsWith('Arrow')) {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      scene?.moveGrip(rect.left + rect.width / 2, rect.top + rect.height / 2,
        e.key === 'ArrowLeft' ? -15 : e.key === 'ArrowRight' ? 15 : 0,
        e.key === 'ArrowUp' ? -15 : e.key === 'ArrowDown' ? 15 : 0, true);
      return;
    }
    if (grabbed ? !confirm && e.key !== 'Escape' && !arrowTargets.has(e.key) : !confirm) return;
    e.preventDefault();
    if (!grabbed) {
      cancelSelection();
      grabbed = true;
      if (state.view === '3d') {
        // The physical die is lifted at once; Enter without an arrow puts it back where it was.
        target = dropZones.find(zone => zone.dataset.tray === (d.tray ?? ''));
        const rect = el.getBoundingClientRect();
        scene?.beginGrip(d, rect.left + rect.width / 2, rect.top + rect.height / 2);
      }
      el.classList.add('held');
      el.setAttribute('aria-pressed', 'true');
      document.body.classList.add('dragging');
      cancelDrag = () => {
        grabbed = false;
        target = undefined;
        el.classList.remove('held');
        el.setAttribute('aria-pressed', 'false');
        clearDropHints();
        scene?.cancelGrip();
        cancelDrag = null;
      };
    } else if (e.key === 'Escape') {
      cancelDrag();
    } else if (confirm) {
      const to = target;
      if (to) moveTo(d, to.dataset.tray);
      scene?.endGrip(d, !to, to === tableEl);
      cancelDrag();
      commit();
      elsOf([d])[0]?.focus();
    } else {
      target = arrowTargets.get(e.key);
      showDropTarget(target);
      const rect = target.getBoundingClientRect();
      scene?.moveGrip(rect.left + rect.width / 2, rect.top + rect.height / 2, 0, 0);
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
const DRAG_SLOP = 12, DRAG_FAST_SLOP = 30, DRAG_DELAY_MS = 150, DRAG_HINT_DELAY_MS = 200;
function pointerDrag(el) {
  el.addEventListener('pointerdown', start => {
    const flat = state.view === '2d';
    // 3D: the pointer grabs the mesh under it; the table target only marks the die's footprint.
    const d = flat || el.die.tray ? el.die : scene?.pick(start.clientX, start.clientY)?.die;
    if (!d) return;
    if (cancelDrag || rolling || start.button > (flat ? 0 : 2)) return;
    start.preventDefault();
    const r = el.getBoundingClientRect();
    // 2D: over a tray the die shrinks to tray size so the gap next to it stays visible.
    const small = flat && parseFloat(getComputedStyle($('main')).getPropertyValue('--tray-die-size') || '3.6rem') * parseFloat(getComputedStyle(document.documentElement).fontSize) / r.width;
    const started = performance.now();
    let moved = false, lastX = start.clientX, lastY = start.clientY;
    let hintTimer = null, hintsVisible = flat;
    const showHints = () => {
      const target = dropTargetAt(lastX, lastY);
      document.body.classList.add('dragging');
      showDropTarget(target);
      // Compute the target without the gap, otherwise the gap pushes its own target away.
      marker.remove();
      if (target && target !== tableEl) target.firstElementChild.insertBefore(marker, trayTarget(target, d, lastX, lastY) ?? null);
    };
    let rotate = start.shiftKey || start.button === 2;
    let secondPointer = null;
    el.classList.add('held'); // lift as soon as the finger is on it
    const holdTimer = start.button === 0 ? setTimeout(() => { cancelDrag?.(); openDie(d); }, 550) : null;
    // 3D: a second finger rotates the gripped die.
    const extraDown = ev => {
      if (ev.pointerId === start.pointerId) return;
      secondPointer = ev.pointerId;
      rotate = true;
      clearTimeout(holdTimer);
    };
    const move = ev => {
      if (ev.pointerId !== start.pointerId && ev.pointerId !== secondPointer) return;
      const dx = ev.clientX - start.clientX, dy = ev.clientY - start.clientY;
      const dist = Math.hypot(dx, dy);
      const threshold = performance.now() - started < DRAG_DELAY_MS ? DRAG_FAST_SLOP : DRAG_SLOP;
      if (!moved && dist < threshold && secondPointer === null) return;
      clearTimeout(holdTimer);
      if (!moved) {
        moved = true;
        cancelSelection();
        if (!flat) hintTimer = setTimeout(() => { hintsVisible = true; showHints(); }, DRAG_HINT_DELAY_MS);
        if (flat) {
          Object.assign(el.style, {
            position: 'fixed', left: r.left + 'px', top: r.top + 'px', width: r.width + 'px',
            zIndex: 10, pointerEvents: 'none',
          });
          el.classList.add('drag');
        } else scene?.beginGrip(d, start.clientX, start.clientY, started);
      }
      const target = dropTargetAt(ev.clientX, ev.clientY);
      const tray = target === tableEl ? undefined : target;
      if (flat) el.style.transform = `translate(${dx}px, ${dy}px) scale(${tray ? small : 1.08})`;
      else scene?.moveGrip(ev.clientX, ev.clientY, ev.clientX - lastX, ev.clientY - lastY, rotate || ev.shiftKey);
      lastX = ev.clientX; lastY = ev.clientY;
      if (hintsVisible) showHints();
    };
    const up = ev => {
      if (ev.pointerId === secondPointer) { secondPointer = null; rotate = start.shiftKey || start.button === 2; return; }
      if (ev.pointerId !== start.pointerId) return;
      clearTimeout(holdTimer);
      clearTimeout(hintTimer);
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', up);
      removeEventListener('pointercancel', up);
      removeEventListener('pointerdown', extraDown);
      cancelDrag = null;
      clearDropHints();
      el.classList.remove('held');
      if (!moved) {
        if (ev.type === 'pointerup' && start.button === 0) tapDie(d);
        else if (ev.type === 'pointerup' && start.button === 2) openDie(d);
        return;
      }
      const target = ev.type === 'pointerup' ? dropTargetAt(ev.clientX, ev.clientY) : null;
      if (target) moveTo(d, target.dataset.tray, target === tableEl ? undefined : trayTarget(target, d, ev.clientX, ev.clientY)?.die);
      scene?.endGrip(d, !target);
      commit();
    };
    cancelDrag = () => up({ pointerId: start.pointerId, type: 'pointercancel' });
    addEventListener('pointermove', move);
    addEventListener('pointerup', up);
    addEventListener('pointercancel', up);
    if (!flat) addEventListener('pointerdown', extraDown);
  });
}

// 2D cup animation: collect → button shakes → throw out. Values are fixed only on the throw.
const play = (el, frames, opts) => el.animate(frames, opts).finished.catch(() => {}); // cancelled = finished
const toButton = el => {
  const a = el.getBoundingClientRect(), b = $('#roll').getBoundingClientRect();
  return `translate(${b.x + b.width / 2 - a.x - a.width / 2}px, ${b.y + b.height / 2 - a.y - a.height / 2}px)`;
};
const tumble = () => `rotate(${Math.round((Math.random() - .5) * 720)}deg)`; // cosmetic only

// One roll for both views and every trigger (button, keyboard, long press, shake): same rules, sounds
// and one history row. 3D throws physically; scene3d.js maps the landed faces to crypto rolls.
// shaken: the phone was shaken – the dice already rattled on the table, so no cup button.
async function doRoll(shaken = false) {
  const flat = state.view === '2d';
  if ((!flat && (!scene || !renderError.hidden || cancelDrag)) || rolling || !state.dice.length || dialogOpen() || (shaken && selected)) return;
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
  if (flat) document.body.classList.toggle('classic', classic);
  const cup = !calm && !shaken && !classic;
  // Cup sound only with the cup animation; when shaken, the rattle already played.
  if (cup) sound('throw');
  else if (!shaken) sound('shuffle'); // spin and reduced motion rattle on the roll
  try {
    if (flat && cup) {
      // 1. Collect: the dice fly into the roll button.
      await Promise.all(elsOf(active).map((el, i) => play(el,
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
    // 2D: shuffled dice scatter over the table instead of each die returning to its old spot.
    compactSlots(state.dice, flat ? shuffle(active) : active);
    if (flat) {
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
    } else {
      render();
      flickDebug.hidden = true;
      await scene.throw(active, { calm, classic: state.animation === 'classic' });
      if (scene.failed) return; // an aborted WebGL throw is not a completed history result
    }
    navigator.vibrate?.(25);
    state.history = [`${state.dice.map(d => d.value).join(' · ')} = ${total(state.dice)}`, ...state.history].slice(0, HISTORY_LIMIT);
  } finally {
    rolling = false;
    $('#roll').disabled = false;
    document.body.classList.remove('rolling', 'sum-pending', 'classic');
    commit();
  }
}

// Roll on release, not only on click: after a long press the browser fires contextmenu and drops the
// click. Only a press that started on the button counts, so dropping a dragged die here does not roll.
// click is only for keyboard and screen readers: pointer clicks already rolled on release.
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
$('#roll').onclick = e => { if (e.detail === 0) doRoll(); };
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
  scene?.renderer.setSurface(state.surface, theme);
  scene?.invalidate();
}
onPick($('#theme'), 'theme', setTheme);
setTheme(document.documentElement.dataset.theme);
onPick($('#animation'), 'animation', animation => {
  state.animation = animation;
  commit();
});
// Switching the view keeps the round as it is; only the presentation changes. Not during a roll.
onPick($('#view'), 'view', view => {
  if (rolling || view === state.view) return;
  interrupt();
  state.view = view;
  if (view === '2d') stopScene();
  commit(); // publishes data-view before the 3D canvas is measured
  if (view === '3d') startScene();
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
async function requestTiltPermission() {
  try {
    const permission = await window.DeviceOrientationEvent?.requestPermission?.();
    if (!permission || permission === 'granted') return;
  } catch {}
  state.tilt = false;
  scene?.setTilt(0, 0);
  commit();
}
onPick($('#tilt'), 'tilt', value => {
  state.tilt = value === 'on';
  if (state.tilt) requestTiltPermission();
  else scene?.setTilt(0, 0);
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

// 2D shake to roll. In 3D the orientation sensor only tilts the physical table.
// Calibration (m/s², including gravity ≈ 9.8): START = jolt to begin, KEEP = still shaking, STILL_MS = rest before the throw.
const SHAKE_START = 22, SHAKE_KEEP = 15, SHAKE_STILL_MS = 350;
let lastMag = 0, shakeTimer = null;
addEventListener('devicemotion', e => {
  const a = e.accelerationIncludingGravity;
  if (state.view !== '2d' || !a || rolling || cancelDrag || selected || dialogOpen() || !state.dice.length) return;
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
addEventListener('deviceorientation', e => {
  if (state.view !== '3d' || !state.tilt || rolling || dialogOpen()) return;
  scene?.setTilt(e.beta, e.gamma, screen.orientation?.angle ?? window.orientation ?? 0);
}, { passive: true });
// iOS sensor permission requires a user gesture; orientation is requested only after opt-in.
addEventListener('click', () => {
  window.DeviceMotionEvent?.requestPermission?.().catch(() => {});
  if (state.tilt) requestTiltPermission();
}, { once: true });

// Startup

if (state.sound) loadSounds();
watchViewport(sizeChanged => {
  if (sizeChanged) {
    interrupt();
    // 2D: the layout moved under a running roll: end its animations instead of flying to stale positions.
    // 3D resizes the camera and collision bounds; the physical roll continues in the new viewport.
    if (rolling && state.view === '2d') for (const animation of document.getAnimations())
      if (animation.effect?.target?.closest('#dice, .tray, #roll')) animation.cancel();
  }
  fitTable();
});
document.addEventListener('visibilitychange', () => { if (document.hidden) interrupt(); });
for (const tray of trayEls) tray.firstElementChild.addEventListener('scroll', () => {
  scene?.refreshEntries(); scene?.invalidate();
}, { passive: true });

commit();
if (state.view === '3d') startScene();
document.documentElement.classList.add('ready');
if (!restored) doRoll();
