import assert from 'node:assert/strict';
import { buildDie, readValue } from './dice3d.js';
import { makeBody, mat3, orientValue, step } from './physics3d.js';
import { DiceScene } from './scene3d.js';

// Rotating during pickup must not reroll a stored result; its visible face must still match.
for (const sides of [4, 6, 100]) {
  const die = { sides, value: sides / 2, tray: 't1' };
  const body = makeBody(buildDie(sides), [0, 2, 0]);
  body.die = die;
  body.faceValues = Uint8Array.from({ length: sides }, (_, i) => sides - i);
  orientValue(body, sides); // currently shows 1, not the value being stored
  body.held = { target: [0, 2, 0], ang: [2, 3, 4] };
  body.vel[1] = -2;
  const scene = { grip: { body }, refreshEntries() {}, invalidate() {} };
  DiceScene.prototype.endGrip.call(scene, die);
  assert.equal(die.value, sides / 2, `D${sides}: storing preserves result`);
  assert.equal(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], die.value, `D${sides}: visible stored number matches result`);
  const position = [...body.pos];
  assert.equal(step([body], 1 / 60, { x: 3, z: 3 }), false);
  assert.deepEqual([...body.pos], position, `D${sides}: tray stays physically fixed`);
}

// Both a bulk Return and repeated taps must avoid the dice already lying on the table.
for (const bulk of [false, true]) {
  const dice = Array.from({ length: 12 }, (_, i) => ({ sides: 6, value: i % 6 + 1, ...(i ? { tray: 'b1' } : {}) }));
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    bounds: { x: 2.7, z: 4.4 }, rect: { width: 390, height: 635 }, bodies: new Map(),
    renderer: { setSurface() {} }, layout() {},
  });
  for (const die of dice) {
    const body = makeBody(buildDie(die.sides), [0, 1, 0]);
    body.die = die; body.tray = die.tray;
    orientValue(body, die.value);
    scene.bodies.set(die, body);
  }
  if (bulk) {
    dice.forEach(die => { delete die.tray; });
    scene.sync(dice, new Map());
  } else for (const die of dice.slice(1)) {
    delete die.tray;
    scene.sync(dice, new Map());
  }
  const bodies = [...scene.bodies.values()];
  for (let i = 0; i < bodies.length; i++) {
    const a = bodies[i];
    assert.equal(readValue(a.shape, mat3(a.quat)), a.die.value);
    assert(Math.abs(a.pos[0]) + a.shape.radius <= scene.bounds.x);
    assert(Math.abs(a.pos[2]) + a.shape.radius <= scene.bounds.z);
    for (const b of bodies.slice(i + 1)) {
      assert(Math.hypot(a.pos[0] - b.pos[0], a.pos[2] - b.pos[2]) > a.shape.radius + b.shape.radius, `${bulk ? 'bulk' : 'tap'} return: no overlaps`);
    }
  }
}

// Keyboard return chooses free space; a pointer drop keeps the user's chosen position.
for (const autoPlace of [false, true]) {
  const die = { sides: 6, value: 3 };
  const body = makeBody(buildDie(6), [0.4, 1.6, 0.4]);
  body.die = die; body.tray = 'b1';
  const blocker = makeBody(buildDie(6), [0, 1, 0]);
  blocker.die = { sides: 6, value: 1 };
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    bounds: { x: 2.7, z: 4.4 }, bodies: new Map([[die, body], [blocker.die, blocker]]),
    grip: { body, originalTray: 'b1' }, refreshEntries() {}, invalidate() {},
  });
  scene.endGrip(die, false, autoPlace);
  if (autoPlace) {
    assert(Math.hypot(body.pos[0], body.pos[2]) > body.shape.radius + blocker.shape.radius);
    assert.equal(readValue(body.shape, mat3(body.quat)), die.value);
  } else {
    assert.equal(body.pos[0], 0.4);
    assert.equal(body.pos[2], 0.4);
  }
}

// Picking up a die can knock a neighbor over, even if the picked-up die ends in a tray.
{
  const picked = makeBody(buildDie(6), [0, 1, 0]);
  picked.die = { sides: 6, value: 2 };
  orientValue(picked, picked.die.value);
  const neighbor = makeBody(buildDie(20), [1, 1, 0]);
  neighbor.die = { sides: 20, value: 1 };
  neighbor.faceValues = Uint8Array.from({ length: 20 }, (_, i) => 20 - i);
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    bodies: new Map([[picked.die, picked], [neighbor.die, neighbor]]),
    active: [neighbor], tableBodies: [neighbor], lastTime: 0, accumulator: 0,
    moveGrip() {}, refreshEntries() {}, invalidate() {}, draw() {},
  });
  scene.beginGrip(picked.die, 0, 0);
  orientValue(neighbor, 6); // the neighbor has landed showing mapped number 15
  picked.die.tray = 'b1';
  scene.endGrip(picked.die);
  scene.tick(16);
  assert.equal(picked.die.value, 2, 'stored die keeps its original result');
  assert.equal(neighbor.die.value, 15, 'bumped neighbor result matches its actual visible face');
}
console.log('scene3d tray values ok');
