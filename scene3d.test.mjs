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
    grip: { body, originalTray: 'b1', vx: 0, vz: 0, lastMotion: performance.now() }, refreshEntries() {}, invalidate() {},
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

// Committing a finished throw must not reposition or straighten an already resting die.
for (const heightChange of [0, -0.001]) {
  const die = { sides: 6, value: 1 };
  const body = makeBody(buildDie(6), [0, 1, 0]);
  orientValue(body, die.value);
  body.die = die;
  const bounds = { x: 2.7, z: 5.4 };
  body.pos[0] = bounds.x - body.shape.inradius;
  body.pos[2] = -1.2;
  body.quat[1] = 0.001;
  const pose = { pos: [...body.pos], quat: [...body.quat] };
  const rect = { left: 0, top: 100, width: 300, height: 600 + heightChange };
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    bounds, rect, bodies: new Map([[die, body]]), targets: new Map(),
    table: { getBoundingClientRect: () => rect },
    topTray: { classList: { contains: () => true } },
    bottomTray: { classList: { contains: () => true } },
    renderer: { setSurface() {} }, refreshEntries() {}, placeTargets() {}, invalidate() {},
  });
  scene.sync([die], new Map());
  assert.deepEqual(body.quat, pose.quat, 'final commit must not straighten the die');
  assert.equal(body.pos[1], pose.pos[1], 'final commit must not lower the die');
  assert(Math.hypot(body.pos[0] - pose.pos[0], body.pos[2] - pose.pos[2]) < 0.001,
    'a wall contact or subpixel layout change must not move the die visibly');
}

// Gentle drops keep the visible result throughout the fall, for every supported shape.
for (const sides of [4, 6, 8, 10, 12, 20, 100]) {
  const die = { sides, value: 1 }, body = makeBody(buildDie(sides), [0, 1, 0]);
  body.die = die;
  body.faceValues = Uint8Array.from({ length: sides }, (_, i) => sides - i);
  orientValue(body, sides);
  body.pos[1] = 1.6; body.vel[0] = 1.9;
  body.held = { target: [...body.pos], ang: [0, 0, 0] };
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    grip: { body, vx: 1.9, vz: 0, lastMotion: performance.now() }, refreshEntries() {}, invalidate() {},
  });
  scene.endGrip(die);
  for (let i = 0; i < 600; i++) {
    step([body], 1 / 120, { x: 4, z: 4 });
    assert.equal(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], 1, `D${sides}: gentle move preserves face`);
    if (body.sleeping) break;
  }
  assert(body.sleeping, `D${sides}: gentle drop comes to rest`);
}

// Plain flicks get rolling spin even when released before the next physics frame.
{
  const originalNow = performance.now;
  let now = 0;
  performance.now = () => now;
  try {
    for (const advance of [false, true]) {
      for (const [vx, vz, pause, tray, cancel, delayedGrab] of [
        [1.9, 0, 0], [2.1, 0, 0], [8, 0, 0], [-8, 0, 0], [0, 8, 0], [0, -8, 0],
        [8, 0, 100], [8, 0, 0, true], [8, 0, 0, false, true],
        [1.9, 0, 0, false, false, true],
      ]) {
        const die = { sides: 6, value: 1 }, body = makeBody(buildDie(6), [0, 1, 0]);
        body.die = die; orientValue(body, 1); body.pos[1] = advance ? body.shape.inradius : 1.6;
        const scene = Object.assign(Object.create(DiceScene.prototype), {
          bodies: new Map([[die, body]]), bounds: { x: 10, z: 10 },
          renderer: { unproject: (x, y) => [x, 1.6, y] },
          refreshEntries() {}, invalidate() {},
        });
        now = delayedGrab ? 100 : 0; scene.beginGrip(die, 0, 0, 0);
        now = 100; scene.moveGrip(vx / 10, vz / 10, vx / 10, vz / 10);
        if (advance) step([body], 1 / 120, scene.bounds);
        now += pause;
        if (tray) die.tray = 't1';
        scene.endGrip(die, cancel);
        assert(body.vel[1] <= 0, 'releasing cannot inherit upward velocity from the pickup motor');
        const speed = Math.hypot(vx, vz);
        if (speed >= 2 && !pause && !tray && !cancel) {
          assert(Math.hypot(body.ang[0], body.ang[2]) > 0,
            'a plain flick imparts tumbling spin without requiring a physics frame');
          const quat = [...body.quat];
          step([body], 1 / 30, scene.bounds);
          assert.notDeepEqual(body.quat, quat, 'the flick visibly rotates the die in flight');
        } else {
          assert.equal(Math.hypot(...body.ang), 0, 'gentle, paused, stored or cancelled drags add no spin');
          assert.equal(readValue(body.shape, mat3(body.quat)), 1, 'the visible value is preserved');
        }
      }
    }
  } finally { performance.now = originalNow; }
}

// Release motion grows continuously from a gentle drop to a bounded strong flick.
{
  const originalNow = performance.now;
  let now = 0, previousSpin = 0, previousSpeed = 0;
  performance.now = () => now;
  try {
    for (const speed of [0, 1.9, 2, 2.01, 2.1, 3, 5, 8, 12, 20, 200]) {
      const die = { sides: 6, value: 1 }, body = makeBody(buildDie(6), [0, 1.6, 0]);
      body.die = die; orientValue(body, 1); body.pos[1] = 1.6;
      const scene = Object.assign(Object.create(DiceScene.prototype), {
        bodies: new Map([[die, body]]), bounds: { x: 100, z: 100 },
        active: [body], tableBodies: [body], lastTime: 0, accumulator: 0,
        renderer: { unproject: (x, y) => [x, 1.6, y] },
        refreshEntries() {}, invalidate() {}, draw() {},
      });
      now = 0; scene.beginGrip(die, 0, 0);
      now = 100; scene.moveGrip(speed / 10, 0, speed / 10, 0);
      scene.endGrip(die);
      const spin = Math.hypot(body.ang[0], body.ang[2]), velocity = Math.hypot(body.vel[0], body.vel[2]);
      assert(spin >= previousSpin && velocity >= previousSpeed, 'stronger gestures do not produce weaker throws');
      assert(spin <= 18, 'even an extreme flick has bounded initial spin');
      if (speed <= 2) assert.equal(spin, 0, 'gentle placement adds no release spin');
      if (speed === 2.1) assert(spin < 0.1 && velocity < 0.1, 'crossing the movement threshold cannot launch a sudden fast throw');
      if (speed === 8) assert(spin > 1 && spin < 8, 'a medium flick has moderate spin');
      if (speed >= 20) assert(spin >= 16, 'a deliberate strong flick can still trigger fair randomization');
      if (speed === 20) {
        body.pos[1] = body.shape.inradius;
        const originalRandom = crypto.getRandomValues;
        let draws = 0;
        crypto.getRandomValues = array => { draws++; return array.fill(0); };
        try {
          scene.tick(116);
          assert(draws > 0, 'a strong floor-level flick cannot lose its randomization to contact damping');
        } finally { crypto.getRandomValues = originalRandom; }
      }
      previousSpin = spin; previousSpeed = velocity;
    }
  } finally { performance.now = originalNow; }
}

// Hand motion tilts the held mesh, but never changes its face or snaps on gentle release.
{
  const originalNow = performance.now;
  let now = 0;
  performance.now = () => now;
  try {
    for (const sides of [4, 6, 8, 10, 12, 20, 100]) {
      const die = { sides, value: 1 }, body = makeBody(buildDie(sides), [0, 1.6, 0]);
      body.die = die; body.faceValues = Uint8Array.from({ length: sides }, (_, i) => sides - i);
      orientValue(body, sides); body.pos[1] = 1.6;
      const originalQuat = [...body.quat];
      const scene = Object.assign(Object.create(DiceScene.prototype), {
        bodies: new Map([[die, body]]), bounds: { x: 10, z: 10 },
        active: [body], tableBodies: [body], lastTime: 0, accumulator: 0,
        renderer: { unproject: (x, y) => [x, 1.6, y] },
        refreshEntries() {}, invalidate() {}, draw() {},
      });
      now = 0; scene.beginGrip(die, 0, 0);
      now = 100; scene.moveGrip(0.15, 0, 0.15, 0);
      for (let i = 0; i < 30; i++) {
        now = 100 + i * 16; scene.tick(now);
        assert.equal(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], 1, `D${sides}: hand motion preserves the visible face`);
      }
      assert.notDeepEqual(body.quat, originalQuat, `D${sides}: the held die is not perfectly planar`);
      const heldQuat = [...body.quat];
      scene.endGrip(die);
      assert.deepEqual(body.quat, heldQuat, `D${sides}: releasing does not snap away the hand tilt`);
      for (let i = 0; i < 600; i++) {
        step([body], 1 / 120, scene.bounds);
        assert.equal(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], 1, `D${sides}: gentle landing preserves the face`);
        if (body.sleeping) break;
      }
      assert(body.sleeping, `D${sides}: the tilted gentle drop settles`);
    }
  } finally { performance.now = originalNow; }
}

// Fast yaw leaves labels readable; fast tumbling randomizes once.
{
  const die = { sides: 6, value: 1 }, body = makeBody(buildDie(6), [0, 1, 0]);
  body.die = die; body.faceValues = Uint8Array.from([6, 5, 4, 3, 2, 1]);
  orientValue(body, 6); body.pos[1] = 3;
  body.held = { target: [...body.pos], ang: [0, 30, 0] };
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    grip: { body, rotated: true, vx: 0, vz: 0, lastMotion: performance.now() },
    bodies: new Map([[die, body]]), active: [body], tableBodies: [body],
    bounds: { x: 10, z: 10 }, lastTime: 0, accumulator: 0,
    refreshEntries() {}, invalidate() {}, draw() {},
  });
  const originalRandom = crypto.getRandomValues;
  let draws = 0;
  crypto.getRandomValues = array => { draws++; return array.fill(0); };
  try {
    scene.endGrip(die);
    assert.equal(draws, 0, 'release does not renumber before fast tumbling');
    body.ang[1] = 30;
    scene.tick(16);
    assert.equal(draws, 0, 'fast yaw keeps the readable upper face');
    body.ang[1] = 0; body.ang[0] = 15.9;
    scene.tick(32);
    assert.equal(draws, 0, 'slow tumbling keeps numbering');
    assert.equal(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], 1);
    body.ang[0] = 16;
    scene.tick(48);
    assert(draws > 0, 'fast tumbling uses the cryptographic generator');
    assert.notEqual(body.faceValues[readValue(body.shape, mat3(body.quat)) - 1], 1);
    const count = draws;
    body.ang[0] = 30;
    scene.tick(64);
    assert.equal(draws, count, 'only one randomization per throw');
  } finally { crypto.getRandomValues = originalRandom; }
}

// Tilt wakes only free dice, rotates with screen orientation and never rerolls their numbering.
{
  const free = makeBody(buildDie(6), [0, 1, 0]), locked = makeBody(buildDie(6), [1, 1, 0]);
  free.die = { sides: 6, value: 1 }; locked.die = { sides: 6, value: 2, tray: 'b1' };
  orientValue(free, 1); orientValue(locked, 2);
  free.randomizeOnSpin = true;
  const lockedPose = { pos: [...locked.pos], quat: [...locked.quat] };
  const scene = Object.assign(Object.create(DiceScene.prototype), {
    gravity: [0, -1, 0], bodies: new Map([[free.die, free], [locked.die, locked]]), invalidate() {},
  });
  scene.setTilt(30, 0);
  assert.equal(free.sleeping, false);
  assert.equal(free.randomizeOnSpin, false);
  assert.equal(locked.sleeping, true);
  assert.deepEqual({ pos: locked.pos, quat: locked.quat }, lockedPose);
  assert(Math.abs(scene.gravity[2] - 0.5) < 1e-9);
  scene.setTilt(30, 0, 90);
  assert(Math.abs(scene.gravity[0] - 0.5) < 1e-9);
  assert(Math.abs(scene.gravity[2]) < 1e-9);
  scene.setTilt(90, 0);
  assert(Math.abs(scene.gravity[1] + Math.cos(75 * Math.PI / 180)) < 1e-9);
  const gravity = [...scene.gravity];
  scene.setTilt(null, 0);
  assert.deepEqual(scene.gravity, gravity, 'missing sensor readings cannot corrupt physics');
  scene.throwing = {};
  scene.setTilt(0, 0);
  assert.deepEqual(scene.gravity, [0, -1, 0], 'disabling tilt levels the table even during a throw');
}

console.log('scene3d tray values ok');
