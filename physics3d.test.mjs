import assert from 'node:assert/strict';
import { buildDie, readValue } from './dice3d.js';
import { makeBody, mat3, orientValue, randomQuat, step } from './physics3d.js';

const SIDES = [4, 6, 8, 10, 12, 20, 100];
const BOUNDS = { x: 3, z: 4 };
const DT = 1 / 120;

// Seeded random so every run is identical.
function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
}

// Height of the lowest point of the rounded die above the table.
function lowest(b) {
  const m = mat3(b.quat), v = b.shape.vertices;
  let y = Infinity;
  for (let i = 0; i < v.length; i += 3) y = Math.min(y, m[1] * v[i] + m[4] * v[i + 1] + m[7] * v[i + 2]);
  return b.pos[1] + y - b.shape.margin;
}

// Cosine between the resting face and the table: 1 when the die lies flat on a face.
function flatness(b) {
  const m = mat3(b.quat);
  return Math.max(...b.shape.faces.map(f => -(m[1] * f.normal[0] + m[4] * f.normal[1] + m[7] * f.normal[2])));
}

const value = b => readValue(b.shape, mat3(b.quat));
const distance = (a, b) => Math.hypot(a.pos[0] - b.pos[0], a.pos[1] - b.pos[1], a.pos[2] - b.pos[2]);

// Step until everything sleeps (20 simulated seconds at most); check() runs after every step.
function settle(bodies, check = () => {}, bounds = BOUNDS) {
  for (let n = 0; n < 2400; n++) {
    const active = step(bodies, DT, bounds);
    check();
    if (!active) return n;
  }
  assert.fail(`dice never settled: ${bodies.filter(b => !b.sleeping).map(b => `d${b.shape.sides}`)}`);
}

// Each die, thrown alone, tumbles and comes to rest flat on a face, touching the table.
for (const sides of SIDES) {
  const random = rng(sides), body = makeBody(buildDie(sides), [-1, 2.5, 0.5], randomQuat(random));
  body.vel = [4, 1, -2];
  body.ang = [(random() - 0.5) * 30, (random() - 0.5) * 30, (random() - 0.5) * 30];
  // Keep this flat-on-table scenario away from walls; a die can legitimately lean against a wall.
  settle([body], () => assert.ok(lowest(body) > -0.02, `d${sides} never sinks into the table`), { x: 10, z: 10 });
  assert.ok(Math.abs(lowest(body)) < 0.01, `d${sides} rests on the table (${lowest(body).toFixed(4)})`);
  assert.ok(flatness(body) > 0.999, `d${sides} lies flat (${flatness(body).toFixed(4)})`);
  assert.ok(value(body) >= 1 && value(body) <= sides);
  assert.equal(step([body], DT, { x: 10, z: 10 }), false, `d${sides} stays asleep`);
}

// A dozen mixed dice thrown into a heap: true-shape contacts, nobody sinks into another die or the table.
function heap(seed) {
  const random = rng(seed);
  const bodies = Array.from({ length: 12 }, (_, i) => {
    const b = makeBody(buildDie(SIDES[i % SIDES.length]), [((i % 4) - 1.5) * 1.4, 1.5 + Math.floor(i / 4) * 1.7, (random() - 0.5) * 2], randomQuat(random));
    b.vel = [-b.pos[0] * 2, random() * 2, (random() - 0.5) * 6];
    b.ang = [(random() - 0.5) * 30, (random() - 0.5) * 30, (random() - 0.5) * 30];
    return b;
  });
  let deepest = 0, floor = 0;
  settle(bodies, () => {
    for (let i = 0; i < bodies.length; i++) {
      floor = Math.min(floor, lowest(bodies[i]));
      // Each die contains its insphere, so closer centers would mean overlapping dice.
      for (let j = i + 1; j < bodies.length; j++) deepest = Math.max(deepest, bodies[i].shape.inradius + bodies[j].shape.inradius - distance(bodies[i], bodies[j]));
    }
  });
  return { bodies, deepest, floor };
}
{
  const { bodies, deepest, floor } = heap(3);
  assert.ok(deepest < 0.03, `dice never interpenetrate (${deepest.toFixed(3)})`);
  assert.ok(floor > -0.02, `no die sinks into the table (${floor.toFixed(3)})`);
  for (const b of bodies) {
    assert.ok(b.sleeping && lowest(b) > -0.01, `d${b.shape.sides} rests`);
    assert.ok(Math.abs(b.pos[0]) < BOUNDS.x && Math.abs(b.pos[2]) < BOUNDS.z, `d${b.shape.sides} stays inside the walls`);
  }
  // Same initial conditions, same landing: bit for bit.
  assert.deepEqual(heap(3).bodies.map(b => [...b.pos, ...b.quat]), bodies.map(b => [...b.pos, ...b.quat]));
}

// A sleeping die is a real obstacle and wakes when struck.
{
  const target = orientValue(makeBody(buildDie(6), [0, 0, 0]), 6);
  const bullet = makeBody(buildDie(6), [-2.4, 0.6, 0]);
  bullet.vel = [10, 0, 0];
  let woke = false, deepest = 0;
  settle([target, bullet], () => {
    woke ||= !target.sleeping;
    deepest = Math.max(deepest, 2 * target.shape.inradius - distance(target, bullet));
  });
  assert.ok(woke, 'struck die wakes');
  assert.ok(Math.hypot(target.pos[0], target.pos[2]) > 0.05, 'struck die is pushed');
  assert.ok(deepest < 0.03, `no pass-through (${deepest.toFixed(3)})`);
}

// A die resting on another wakes and falls when its support is removed; a resting stack stays asleep.
{
  const base = orientValue(makeBody(buildDie(6), [0, 0, 0]), 1);
  const top = makeBody(buildDie(6), [0.1, 3 * base.shape.inradius, 0]);
  settle([base, top]);
  assert.ok(Math.abs(top.pos[1] - 3 * base.shape.inradius) < 0.01, 'stacked die rests on the other');
  assert.equal(step([base, top], DT, BOUNDS), false, 'a resting stack stays asleep');
  base.dead = true;
  settle([base, top]);
  assert.ok(Math.abs(top.pos[1] - base.shape.inradius) < 0.01, 'falls once its support is gone');
}

// Held: follows its target and spin, keeps the throw when released, then settles.
{
  const body = makeBody(buildDie(20), [0, 0.6, 0]);
  body.held = { target: [0, 1.5, 0], ang: [0, 4, 0] };
  for (let i = 0; i < 60; i++) step([body], DT, BOUNDS);
  assert.ok(distance(body, { pos: body.held.target }) < 0.02, 'held die reaches its target');
  assert.ok(Math.abs(body.ang[1] - 4) < 1e-9, 'held die spins as told');
  for (let i = 0; i < 30; i++) { body.held.target[0] += 3 * DT; step([body], DT, BOUNDS); }
  assert.ok(Math.abs(body.vel[0] - 3) < 0.5, `held die moves with the target (${body.vel[0].toFixed(2)})`);
  body.held = null;
  const x = body.pos[0];
  settle([body]);
  assert.ok(body.pos[0] > x + 0.3, 'released die keeps its throw');
  assert.ok(flatness(body) > 0.999);
}

// orientValue: any value of any die, upright and resting on the table without moving x/z; it really rests.
for (const sides of SIDES) {
  const shape = buildDie(sides), random = rng(sides * 13);
  for (const f of shape.faces) {
    const b = orientValue(makeBody(shape, [1.5, 3, -0.5], randomQuat(random)), f.value);
    assert.equal(value(b), f.value, `d${sides} shows ${f.value}`);
    assert.ok(Math.abs(lowest(b)) < 1e-5 && flatness(b) > 1 - 1e-9, `d${sides} ${f.value} sits on the table`);
    assert.deepEqual([b.pos[0], b.pos[2], b.sleeping], [1.5, -0.5, true]);
  }
  const b = orientValue(makeBody(shape, [0, 1, 0], randomQuat(random)), sides);
  b.sleeping = false;
  settle([b]);
  assert.equal(value(b), sides, `d${sides} oriented die stays put`);
  assert.ok(Math.abs(b.pos[1] - shape.inradius) < 0.005 && Math.hypot(b.pos[0], b.pos[2]) < 0.01);
}

// Empty tray space is a real extension, with independent walls beyond the original table.
for (const direction of [-1, 1]) {
  const bounds = { x: 3, z: 4, minZ: direction < 0 ? -6 : -4, maxZ: direction > 0 ? 6 : 4 };
  const body = orientValue(makeBody(buildDie(6), [0, 1, direction * 5]), 1);
  body.sleeping = false;
  body.vel[2] = direction * 4;
  settle([body], () => {
    const m = mat3(body.quat), v = body.shape.vertices;
    for (let i = 0; i < v.length; i += 3) {
      const z = body.pos[2] + m[2] * v[i] + m[5] * v[i + 1] + m[8] * v[i + 2];
      assert(z - body.shape.margin >= bounds.minZ - 0.02);
      assert(z + body.shape.margin <= bounds.maxZ + 0.02);
    }
  }, bounds);
  assert(direction * body.pos[2] > 4.4, 'die remains in the empty tray extension');
}

// The same slope leaves a cube stable but rolls the smaller-faced D100; steeper slopes slide a cube.
const slopeBodies = [];
for (const [sides, degrees] of [[6, 15], [100, 15], [6, 35]]) {
  const body = orientValue(makeBody(buildDie(sides), [0, 1, 0]), 1);
  const angle = degrees * Math.PI / 180, gravity = [Math.sin(angle), -Math.cos(angle), 0];
  body.sleeping = false;
  for (let i = 0; i < 240; i++) step([body], DT, { x: 100, z: 100 }, gravity);
  slopeBodies.push(body);
}
assert(Math.abs(slopeBodies[0].pos[0]) < 0.01, 'cube sticks below its friction/tipping limit');
assert(slopeBodies[1].pos[0] > 0.1, 'D100 rolls on a slope that leaves the cube stable');
assert(slopeBodies[2].pos[0] > 0.2, 'cube slides when gravity exceeds friction');

console.log('physics3d ok');
