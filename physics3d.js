// physics3d.js — rigid-body dice. A die collides as its rounded core (convex core ⊕ ball of radius
// shape.margin), exactly the rendered shape: GJK distance between the cores, reference-face clipping for
// resting manifolds, vertex contacts with the table and the walls, sequential impulses with Coulomb friction
// and restitution, and sleeping. Deterministic and allocation-free per step: randomness lives only in the
// initial conditions.
const GRAVITY = 32;            // world units/s²; a d6 is 0.9 wide
const DOWN = [0, -1, 0];
const SUBSTEP = 1 / 240;
const ITERATIONS = 10;
const SPECULATIVE = 0.03;      // contacts start this far before touching, so fast dice cannot pass through
const SLOP = 0.002;            // tolerated penetration, keeps resting contacts calm
const BOUNCE = 0.8;            // slower impacts do not bounce (no resting jitter)
const TABLE = { e: 0.3, mu: 0.55 }, WALL = { e: 0.5, mu: 0.25 }, DIE = { e: 0.35, mu: 0.3 };
const SLEEP_SPEED = 0.1, SLEEP_SPIN = 0.3, SLEEP_TIME = 0.3;
const WAKE_SPEED = 0.25;       // a sleeping die hit by a slower die stays asleep and holds it like a wall
const HOLD_GAIN = 18, HOLD_MAX = 25;

// Column-major rotation matrix of a unit quaternion [x, y, z, w], the layout readValue expects.
export function mat3(q, out = new Array(9)) {
  const x = q[0], y = q[1], z = q[2], w = q[3];
  out[0] = 1 - 2 * (y * y + z * z); out[1] = 2 * (x * y + z * w); out[2] = 2 * (x * z - y * w);
  out[3] = 2 * (x * y - z * w); out[4] = 1 - 2 * (x * x + z * z); out[5] = 2 * (y * z + x * w);
  out[6] = 2 * (x * z + y * w); out[7] = 2 * (y * z - x * w); out[8] = 1 - 2 * (x * x + y * y);
  return out;
}

// Uniformly random orientation (Shoemake), from any random() in [0,1).
export function randomQuat(random = Math.random) {
  const u1 = random(), u2 = random() * Math.PI * 2, u3 = random() * Math.PI * 2;
  const a = Math.sqrt(1 - u1), b = Math.sqrt(u1);
  return [a * Math.sin(u2), a * Math.cos(u2), b * Math.sin(u3), b * Math.cos(u3)];
}

export function makeBody(shape, pos, quat = [0, 0, 0, 1]) {
  const l = Math.hypot(...quat), k = (shape.inradius + shape.radius) / 2;
  const body = {
    shape, pos: [...pos], quat: quat.map(c => c / l), vel: [0, 0, 0], ang: [0, 0, 0],
    sleeping: false, sleepTimer: 0, held: null, guided: false, randomizeOnSpin: false, dead: false,
    // Equal masses; inertia of a ball between in- and circumradius (exact enough for these round solids).
    invMass: 1, invInertia: 1 / (0.4 * k * k), im: 1, ii: 0, touch: false,
    rest: null, stamp: 0, // dice it slept against
    m: new Float64Array(9), world: new Float64Array(shape.vertices.length), // current rotation and core vertices
  };
  place(body);
  return body;
}

function place(b) {
  const m = mat3(b.quat, b.m), v = b.shape.vertices, w = b.world, px = b.pos[0], py = b.pos[1], pz = b.pos[2];
  for (let i = 0; i < v.length; i += 3) {
    const x = v[i], y = v[i + 1], z = v[i + 2];
    w[i] = m[0] * x + m[3] * y + m[6] * z + px;
    w[i + 1] = m[1] * x + m[4] * y + m[7] * z + py;
    w[i + 2] = m[2] * x + m[5] * y + m[8] * z + pz;
  }
}

// Turn a die so `value` is up (a d4: the face it rests on, its apex reading), keeping its heading and x/z,
// and set it down at rest on the table. For restoring saved results and rolls without motion.
export function orientValue(body, value) {
  const shape = body.shape, face = shape.faces.find(f => f.value === value);
  if (!face) throw new RangeError(`d${shape.sides} has no ${value}`);
  const m = mat3(body.quat, body.m), n = face.normal, t = shape.bottomRead ? -1 : 1;
  const wx = m[0] * n[0] + m[3] * n[1] + m[6] * n[2], wy = m[1] * n[0] + m[4] * n[1] + m[7] * n[2], wz = m[2] * n[0] + m[5] * n[1] + m[8] * n[2];
  // Shortest rotation of the face normal onto (0, t, 0): axis w × (0, t, 0).
  const ax = -wz * t, az = wx * t, s = Math.hypot(ax, az), c = wy * t;
  const angle = Math.atan2(s, c), h = Math.sin(angle / 2);
  const r = s > 1e-9 ? [ax / s * h, 0, az / s * h, Math.cos(angle / 2)] : c > 0 ? [0, 0, 0, 1] : [1, 0, 0, 0];
  const q = body.quat, x = q[0], y = q[1], z = q[2], w = q[3];
  q[0] = r[3] * x + r[0] * w + r[1] * z - r[2] * y;
  q[1] = r[3] * y - r[0] * z + r[1] * w + r[2] * x;
  q[2] = r[3] * z + r[0] * y - r[1] * x + r[2] * w;
  q[3] = r[3] * w - r[0] * x - r[1] * y - r[2] * z;
  const l = Math.hypot(q[0], q[1], q[2], q[3]);
  for (let i = 0; i < 4; i++) q[i] /= l;
  body.pos[1] = shape.inradius;
  body.vel.fill(0); body.ang.fill(0);
  body.sleeping = true; body.sleepTimer = 0; body.rest = null; body.guided = false; body.randomizeOnSpin = false;
  place(body);
  return body;
}

// Advance by dt seconds (fixed substeps). bounds: half extents x/z, optional asymmetric minZ/maxZ.
// The table top is y = 0 and walls are infinitely high. Held bodies chase held.target with spin held.ang and keep that
// velocity when released. Returns whether anything still moves.
let stamp = 0;
export function step(bodies, dt, bounds, gravity = DOWN) {
  // A sleeping die wakes when a die it rested against is gone (dead or no longer simulated).
  stamp++;
  for (const b of bodies) b.stamp = stamp;
  for (const b of bodies) {
    if (!b.sleeping || !b.rest) continue;
    for (const r of b.rest) if (r.dead || r.stamp !== stamp) { rouse(b); break; }
  }
  if (dt > 0) {
    const n = Math.ceil(Math.min(dt, 0.05) / SUBSTEP - 1e-9), h = Math.min(dt, 0.05) / n;
    for (let i = 0; i < n; i++) substep(bodies, h, bounds, gravity);
  }
  let active = false;
  for (const b of bodies) {
    if (b.dead || (b.sleeping && !b.held)) continue;
    place(b);
    active = true;
  }
  return active;
}

function rouse(b) {
  b.sleeping = false; b.sleepTimer = 0; b.rest = null;
  b.im = b.invMass; b.ii = b.invInertia;
}

function substep(bodies, dt, bounds, gravity) {
  const gx = GRAVITY * gravity[0] * dt, gy = GRAVITY * gravity[1] * dt, gz = GRAVITY * gravity[2] * dt;
  for (const b of bodies) {
    if (b.dead) continue;
    if (b.held) {
      b.sleeping = false; b.sleepTimer = 0;
      const t = b.held.target;
      let x = (t[0] - b.pos[0]) * HOLD_GAIN, y = (t[1] - b.pos[1]) * HOLD_GAIN, z = (t[2] - b.pos[2]) * HOLD_GAIN;
      const s = Math.hypot(x, y, z);
      if (s > HOLD_MAX) { x *= HOLD_MAX / s; y *= HOLD_MAX / s; z *= HOLD_MAX / s; }
      b.vel[0] = x; b.vel[1] = y; b.vel[2] = z;
      if (b.held.ang) { b.ang[0] = b.held.ang[0]; b.ang[1] = b.held.ang[1]; b.ang[2] = b.held.ang[2]; }
      else { const k = Math.max(0, 1 - 8 * dt); b.ang[0] *= k; b.ang[1] *= k; b.ang[2] *= k; }
    } else if (!b.sleeping) { b.vel[0] += gx; b.vel[1] += gy; b.vel[2] += gz; }
    b.im = b.sleeping ? 0 : b.invMass;
    b.ii = b.sleeping || b.held || b.guided ? 0 : b.invInertia;
    b.touch = false;
    place(b);
  }

  nc = ng = 0;
  // ponytail: all-pairs bounding-sphere broadphase, 66 pairs at 12 dice; sweep-and-prune if counts grow.
  for (let i = 0; i < bodies.length; i++) {
    const A = bodies[i];
    if (A.dead) continue;
    for (let j = i + 1; j < bodies.length; j++) {
      const B = bodies[j];
      if (B.dead || (A.sleeping && B.sleeping)) continue;
      const dx = B.pos[0] - A.pos[0], dy = B.pos[1] - A.pos[1], dz = B.pos[2] - A.pos[2], r = A.shape.radius + B.shape.radius + SPECULATIVE;
      if (dx * dx + dy * dy + dz * dz < r * r) collide(A, B, dt);
    }
  }
  for (const b of bodies) if (!b.dead && !b.sleeping) planes(b, bounds.x, bounds.minZ ?? -bounds.z, bounds.maxZ ?? bounds.z, dt);

  for (let it = 0; it < ITERATIONS; it++) for (let i = 0; i < nc; i++) solve(contacts[i]);

  for (const b of bodies) {
    if (b.dead || b.sleeping) continue;
    b.pos[0] += b.vel[0] * dt; b.pos[1] += b.vel[1] * dt; b.pos[2] += b.vel[2] * dt;
    spin(b.quat, b.ang, dt);
  }

  // Push overlapping bodies apart by most of the penetration (positions only, adds no energy).
  for (let i = 0; i < ng; i++) {
    const g = groups[i], ia = g.a ? g.a.im : 0, ib = g.b.im, total = ia + ib;
    if (!total) continue;
    const k = 0.8 * (g.depth - SLOP) / total;
    g.b.pos[0] += g.nx * k * ib; g.b.pos[1] += g.ny * k * ib; g.b.pos[2] += g.nz * k * ib;
    if (g.a) { g.a.pos[0] -= g.nx * k * ia; g.a.pos[1] -= g.ny * k * ia; g.a.pos[2] -= g.nz * k * ia; }
  }

  for (const b of bodies) {
    if (b.dead || b.sleeping || b.held) continue;
    const v = b.vel, w = b.ang;
    if (b.guided && b.touch && Math.abs(v[1]) < SLEEP_SPEED) b.guided = false;
    if (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < SLEEP_SPEED ** 2 && w[0] * w[0] + w[1] * w[1] + w[2] * w[2] < SLEEP_SPIN ** 2) {
      b.sleepTimer += dt;
      if (b.sleepTimer >= SLEEP_TIME) { b.sleeping = true; v.fill(0); w.fill(0); b.rest = supports(b); }
    } else b.sleepTimer = 0;
  }
}

function supports(b) {
  let list = null;
  for (let i = 0; i < nc; i++) {
    const c = contacts[i], other = c.b === b ? c.a : c.a === b ? c.b : null;
    if (other && !(list ??= []).includes(other)) list.push(other);
  }
  return list;
}

// q += ½ (ω, 0) ⊗ q · dt, renormalized; ω in world space.
function spin(q, w, dt) {
  const x = q[0], y = q[1], z = q[2], s = q[3], k = 0.5 * dt;
  q[0] = x + k * (w[0] * s + w[1] * z - w[2] * y);
  q[1] = y + k * (w[1] * s + w[2] * x - w[0] * z);
  q[2] = z + k * (w[2] * s + w[0] * y - w[1] * x);
  q[3] = s - k * (w[0] * x + w[1] * y + w[2] * z);
  const l = Math.hypot(q[0], q[1], q[2], q[3]);
  q[0] /= l; q[1] /= l; q[2] /= l; q[3] /= l;
}

// ---- contacts: pooled, normal from a to b (a null = static table or wall) ----
const contacts = [], groups = [];
let nc = 0, ng = 0, rvx = 0, rvy = 0, rvz = 0;

function relVel(c) {
  const b = c.b, v = b.vel, w = b.ang;
  rvx = v[0] + w[1] * c.rbz - w[2] * c.rby;
  rvy = v[1] + w[2] * c.rbx - w[0] * c.rbz;
  rvz = v[2] + w[0] * c.rby - w[1] * c.rbx;
  const a = c.a;
  if (a) {
    const va = a.vel, wa = a.ang;
    rvx -= va[0] + wa[1] * c.raz - wa[2] * c.ray;
    rvy -= va[1] + wa[2] * c.rax - wa[0] * c.raz;
    rvz -= va[2] + wa[0] * c.ray - wa[1] * c.rax;
  }
}

// Inverse effective mass along a direction (scalar inertia: im + ii·|r×d|² per body).
function mass(c, x, y, z) {
  const b = c.b;
  let cx = c.rby * z - c.rbz * y, cy = c.rbz * x - c.rbx * z, cz = c.rbx * y - c.rby * x;
  let k = b.im + b.ii * (cx * cx + cy * cy + cz * cz);
  if (c.a) {
    cx = c.ray * z - c.raz * y; cy = c.raz * x - c.rax * z; cz = c.rax * y - c.ray * x;
    k += c.a.im + c.a.ii * (cx * cx + cy * cy + cz * cz);
  }
  return k;
}

// s: separation of the surfaces at the point (negative = penetrating).
function addContact(a, b, px, py, pz, nx, ny, nz, s, material, dt) {
  const c = contacts[nc] ?? (contacts[nc] = {});
  nc++;
  c.a = a; c.b = b; c.nx = nx; c.ny = ny; c.nz = nz; c.mu = material.mu;
  c.rax = a ? px - a.pos[0] : 0; c.ray = a ? py - a.pos[1] : 0; c.raz = a ? pz - a.pos[2] : 0;
  c.rbx = px - b.pos[0]; c.rby = py - b.pos[1]; c.rbz = pz - b.pos[2];
  let tx, ty, tz;
  if (Math.abs(nx) > 0.57) { const l = Math.hypot(nx, ny); tx = ny / l; ty = -nx / l; tz = 0; }
  else { const l = Math.hypot(ny, nz); tx = 0; ty = nz / l; tz = -ny / l; }
  c.t1x = tx; c.t1y = ty; c.t1z = tz;
  c.t2x = ny * tz - nz * ty; c.t2y = nz * tx - nx * tz; c.t2z = nx * ty - ny * tx;
  c.kn = mass(c, nx, ny, nz); c.kt1 = mass(c, tx, ty, tz); c.kt2 = mass(c, c.t2x, c.t2y, c.t2z);
  relVel(c);
  const vn = rvx * nx + rvy * ny + rvz * nz;
  // Impacts reaching contact within this substep bounce; otherwise close the gap without bouncing.
  c.target = vn < -BOUNCE && s + vn * dt < 0 ? -material.e * vn : s > 0 ? -s / dt : 0;
  c.ln = c.lt1 = c.lt2 = 0;
}

function addGroup(a, b, nx, ny, nz, depth) {
  if (depth <= SLOP) return;
  const g = groups[ng] ?? (groups[ng] = {});
  ng++;
  g.a = a; g.b = b; g.nx = nx; g.ny = ny; g.nz = nz; g.depth = depth;
}

function impulse(c, x, y, z) {
  const b = c.b, a = c.a;
  if (b.im) {
    b.vel[0] += x * b.im; b.vel[1] += y * b.im; b.vel[2] += z * b.im;
    b.ang[0] += (c.rby * z - c.rbz * y) * b.ii; b.ang[1] += (c.rbz * x - c.rbx * z) * b.ii; b.ang[2] += (c.rbx * y - c.rby * x) * b.ii;
  }
  if (a && a.im) {
    a.vel[0] -= x * a.im; a.vel[1] -= y * a.im; a.vel[2] -= z * a.im;
    a.ang[0] -= (c.ray * z - c.raz * y) * a.ii; a.ang[1] -= (c.raz * x - c.rax * z) * a.ii; a.ang[2] -= (c.rax * y - c.ray * x) * a.ii;
  }
}

function solve(c) {
  relVel(c);
  let l = Math.max(0, c.ln + (c.target - (rvx * c.nx + rvy * c.ny + rvz * c.nz)) / c.kn), d = l - c.ln;
  c.ln = l;
  if (d) impulse(c, d * c.nx, d * c.ny, d * c.nz);
  const max = c.mu * c.ln;
  relVel(c);
  l = Math.min(max, Math.max(-max, c.lt1 - (rvx * c.t1x + rvy * c.t1y + rvz * c.t1z) / c.kt1)); d = l - c.lt1;
  c.lt1 = l;
  if (d) impulse(c, d * c.t1x, d * c.t1y, d * c.t1z);
  relVel(c);
  l = Math.min(max, Math.max(-max, c.lt2 - (rvx * c.t2x + rvy * c.t2y + rvz * c.t2z) / c.kt2)); d = l - c.lt2;
  c.lt2 = l;
  if (d) impulse(c, d * c.t2x, d * c.t2y, d * c.t2z);
}

// Core vertices near the table or a wall: every vertex of a resting face is a contact, so dice lie flat.
function planes(b, bx, minZ, maxZ, dt) {
  const w = b.world, r = b.shape.margin;
  let d0 = 0, d1 = 0, d2 = 0, d3 = 0, d4 = 0;
  for (let i = 0; i < w.length; i += 3) {
    const x = w[i], y = w[i + 1], z = w[i + 2];
    let s = y - r;
    if (s < SPECULATIVE) { addContact(null, b, x, y - r, z, 0, 1, 0, s, TABLE, dt); b.touch = true; d0 = Math.max(d0, -s); }
    s = bx - x - r;
    if (s < SPECULATIVE) { addContact(null, b, x + r, y, z, -1, 0, 0, s, WALL, dt); d1 = Math.max(d1, -s); }
    s = bx + x - r;
    if (s < SPECULATIVE) { addContact(null, b, x - r, y, z, 1, 0, 0, s, WALL, dt); d2 = Math.max(d2, -s); }
    s = maxZ - z - r;
    if (s < SPECULATIVE) { addContact(null, b, x, y, z + r, 0, 0, -1, s, WALL, dt); d3 = Math.max(d3, -s); }
    s = z - minZ - r;
    if (s < SPECULATIVE) { addContact(null, b, x, y, z - r, 0, 0, 1, s, WALL, dt); d4 = Math.max(d4, -s); }
  }
  addGroup(null, b, 0, 1, 0, d0);
  addGroup(null, b, -1, 0, 0, d1); addGroup(null, b, 1, 0, 0, d2);
  addGroup(null, b, 0, 0, -1, d3); addGroup(null, b, 0, 0, 1, d4);
}

// ---- die against die ----
const moving = b => b.held || b.vel[0] ** 2 + b.vel[1] ** 2 + b.vel[2] ** 2 + (b.ang[0] ** 2 + b.ang[1] ** 2 + b.ang[2] ** 2) * b.shape.radius ** 2 > WAKE_SPEED ** 2;

function collide(A, B, dt) {
  const rA = A.shape.margin, rB = B.shape.margin, d = gjk(A, B);
  if (d >= 0 && d - rA - rB >= SPECULATIVE) return;
  if (A.sleeping && moving(B)) rouse(A);
  else if (B.sleeping && moving(A)) rouse(B);
  if (d >= 0) {
    const nx = (PB[0] - PA[0]) / d, ny = (PB[1] - PA[1]) / d, nz = (PB[2] - PA[2]) / d;
    // A face of either core lying along the normal carries a multi-point manifold (stable stacking);
    // the GJK point is added when the clipped face misses the closest feature.
    const fa = bestFace(A, nx, ny, nz), da = best, fb = bestFace(B, -nx, -ny, -nz), db = best, s = d - rA - rB;
    const made = Math.max(da, db) > 0.9 && (da >= db ? manifold(A, fa, B, true, dt) : manifold(B, fb, A, false, dt));
    if (made && closest < s + 0.005) return;
    const k = (rA - rB) / 2;
    addContact(A, B, (PA[0] + PB[0]) / 2 + nx * k, (PA[1] + PB[1]) / 2 + ny * k, (PA[2] + PB[2]) / 2 + nz * k, nx, ny, nz, s, DIE, dt);
    addGroup(A, B, nx, ny, nz, -s);
    return;
  }
  // Cores overlap (a very fast impact): least-penetration face axis, then the same clipping.
  scan(B, A, scan(A, B, Infinity));
  const ref = satRef;
  if (manifold(ref, satFace, ref === A ? B : A, ref === A, dt)) return;
  // Nothing clipped (degenerate overlap): push apart along the centers.
  const dx = B.pos[0] - A.pos[0], dy = B.pos[1] - A.pos[1], dz = B.pos[2] - A.pos[2], l = Math.hypot(dx, dy, dz) || 1;
  addContact(A, B, A.pos[0] + dx / 2, A.pos[1] + dy / 2, A.pos[2] + dz / 2, dx / l, dy / l, dz / l, -SLOP, DIE, dt);
  addGroup(A, B, dx / l, dy / l, dz / l, 2 * SLOP);
}

let best = 0, satRef = null, satFace = -1, closest = 0; // closest: smallest separation in the last manifold

// Face of the body whose world normal is closest to direction (x, y, z); its cosine in `best`.
function bestFace(body, x, y, z) {
  const m = body.m, lx = m[0] * x + m[1] * y + m[2] * z, ly = m[3] * x + m[4] * y + m[5] * z, lz = m[6] * x + m[7] * y + m[8] * z;
  const faces = body.shape.faces;
  let bi = 0;
  best = -Infinity;
  for (let i = 0; i < faces.length; i++) {
    const n = faces[i].normal, d = n[0] * lx + n[1] * ly + n[2] * lz;
    if (d > best) { best = d; bi = i; }
  }
  return bi;
}

// Separating-axis scan over P's face normals: the axis where Q penetrates P's face plane least.
function scan(P, Q, least) {
  const faces = P.shape.faces, m = P.m, pw = P.world, qw = Q.world;
  for (let f = 0; f < faces.length; f++) {
    const n = faces[f].normal, o = faces[f].idx[0] * 3;
    const x = m[0] * n[0] + m[3] * n[1] + m[6] * n[2], y = m[1] * n[0] + m[4] * n[1] + m[7] * n[2], z = m[2] * n[0] + m[5] * n[1] + m[8] * n[2];
    let min = Infinity;
    for (let k = 0; k < qw.length; k += 3) min = Math.min(min, qw[k] * x + qw[k + 1] * y + qw[k + 2] * z);
    const depth = pw[o] * x + pw[o + 1] * y + pw[o + 2] * z - min;
    if (depth < least) { least = depth; satRef = P; satFace = f; }
  }
  return least;
}

const CA = new Float64Array(192), CB = new Float64Array(192);

function clipPoly(src, dst, count, nx, ny, nz, off) {
  let out = 0;
  for (let k = 0; k < count; k++) {
    const i = 3 * k, j = 3 * ((k + 1) % count);
    const dp = src[i] * nx + src[i + 1] * ny + src[i + 2] * nz - off, dq = src[j] * nx + src[j + 1] * ny + src[j + 2] * nz - off;
    if (dp <= 0) { dst[3 * out] = src[i]; dst[3 * out + 1] = src[i + 1]; dst[3 * out + 2] = src[i + 2]; out++; }
    if (dp * dq < 0) {
      const t = dp / (dp - dq);
      dst[3 * out] = src[i] + (src[j] - src[i]) * t;
      dst[3 * out + 1] = src[i + 1] + (src[j + 1] - src[i + 1]) * t;
      dst[3 * out + 2] = src[i + 2] + (src[j + 2] - src[i + 2]) * t;
      out++;
    }
  }
  return out;
}

// Contacts of the other body's most opposed face clipped to the reference face's prism. Returns their count.
function manifold(ref, fi, inc, refIsA, dt) {
  const A = refIsA ? ref : inc, B = refIsA ? inc : ref, sg = refIsA ? 1 : -1;
  const face = ref.shape.faces[fi], m = ref.m, fn = face.normal, rw = ref.world, iw = inc.world, ring = face.idx;
  const nx = m[0] * fn[0] + m[3] * fn[1] + m[6] * fn[2], ny = m[1] * fn[0] + m[4] * fn[1] + m[7] * fn[2], nz = m[2] * fn[0] + m[5] * fn[1] + m[8] * fn[2];
  const o = ring[0] * 3, h = rw[o] * nx + rw[o + 1] * ny + rw[o + 2] * nz;
  const poly = inc.shape.faces[bestFace(inc, -nx, -ny, -nz)].idx;
  let src = CA, dst = CB, count = poly.length;
  for (let k = 0; k < count; k++) { const j = poly[k] * 3; src[3 * k] = iw[j]; src[3 * k + 1] = iw[j + 1]; src[3 * k + 2] = iw[j + 2]; }
  for (let k = 0; k < ring.length && count; k++) {
    const p = ring[k] * 3, q = ring[(k + 1) % ring.length] * 3;
    const ex = rw[q] - rw[p], ey = rw[q + 1] - rw[p + 1], ez = rw[q + 2] - rw[p + 2];
    const sx = ey * nz - ez * ny, sy = ez * nx - ex * nz, sz = ex * ny - ey * nx; // edge × normal points out of the face
    count = clipPoly(src, dst, count, sx, sy, sz, sx * rw[p] + sy * rw[p + 1] + sz * rw[p + 2]);
    const t = src; src = dst; dst = t;
  }
  const rr = ref.shape.margin + inc.shape.margin;
  let made = 0, deepest = 0;
  closest = Infinity;
  for (let k = 0; k < count; k++) {
    const x = src[3 * k], y = src[3 * k + 1], z = src[3 * k + 2], gap = x * nx + y * ny + z * nz - h, s = gap - rr;
    if (s >= SPECULATIVE) continue;
    addContact(A, B, x - nx * gap / 2, y - ny * gap / 2, z - nz * gap / 2, sg * nx, sg * ny, sg * nz, s, DIE, dt);
    made++;
    deepest = Math.max(deepest, -s);
    closest = Math.min(closest, s);
  }
  if (made) addGroup(A, B, sg * nx, sg * ny, sg * nz, deepest);
  return made;
}

// ---- GJK distance between two cores (Ericson, Real-Time Collision Detection 9.5) ----
const SW = new Float64Array(12), SA = new Int32Array(4), SB = new Int32Array(4), SL = new Float64Array(4);
const TW = new Float64Array(12), TA = new Int32Array(4), TB = new Int32Array(4);
const RI = new Int32Array(3), RL = new Float64Array(3), BI = new Int32Array(3), BL = new Float64Array(3);
const PA = new Float64Array(3), PB = new Float64Array(3);
let sn = 0, rn = 0, vx = 0, vy = 0, vz = 0;

function support(w, x, y, z) {
  let top = -Infinity, bi = 0;
  for (let i = 0; i < w.length; i += 3) {
    const d = w[i] * x + w[i + 1] * y + w[i + 2] * z;
    if (d > top) { top = d; bi = i; }
  }
  return bi;
}

// Distance between the cores with closest points in PA, PB; -1 when the cores overlap.
function gjk(A, B) {
  const wa = A.world, wb = B.world;
  vx = A.pos[0] - B.pos[0]; vy = A.pos[1] - B.pos[1]; vz = A.pos[2] - B.pos[2];
  if (!vx && !vy && !vz) vx = 1;
  sn = 0;
  for (let it = 0; it < 64; it++) {
    const ia = support(wa, -vx, -vy, -vz), ib = support(wb, vx, vy, vz);
    const wx = wa[ia] - wb[ib], wy = wa[ia + 1] - wb[ib + 1], wz = wa[ia + 2] - wb[ib + 2], vv = vx * vx + vy * vy + vz * vz;
    if (sn && vv - (vx * wx + vy * wy + vz * wz) <= 1e-9 * vv) break; // no closer point exists
    let seen = false;
    for (let k = 0; k < sn; k++) if (SA[k] === ia && SB[k] === ib) seen = true;
    if (seen) break;
    SW[3 * sn] = wx; SW[3 * sn + 1] = wy; SW[3 * sn + 2] = wz; SA[sn] = ia; SB[sn] = ib; sn++;
    if (!reduce() || vx * vx + vy * vy + vz * vz < 1e-14) return -1;
  }
  PA.fill(0); PB.fill(0);
  for (let k = 0; k < sn; k++) for (let c = 0; c < 3; c++) { PA[c] += SL[k] * wa[SA[k] + c]; PB[c] += SL[k] * wb[SB[k] + c]; }
  return Math.sqrt(vx * vx + vy * vy + vz * vz);
}

// Closest point of the simplex to the origin; keep only the vertices that span it. False: origin inside.
function reduce() {
  if (sn === 1) { RI[0] = 0; RL[0] = 1; rn = 1; }
  else if (sn === 2) segment(0, 1);
  else if (sn === 3) triangle(0, 1, 2);
  else if (!tetrahedron()) return false;
  for (let k = 0; k < rn; k++) {
    const i = RI[k];
    TW[3 * k] = SW[3 * i]; TW[3 * k + 1] = SW[3 * i + 1]; TW[3 * k + 2] = SW[3 * i + 2]; TA[k] = SA[i]; TB[k] = SB[i];
  }
  vx = vy = vz = 0;
  for (let k = 0; k < rn; k++) {
    SW[3 * k] = TW[3 * k]; SW[3 * k + 1] = TW[3 * k + 1]; SW[3 * k + 2] = TW[3 * k + 2]; SA[k] = TA[k]; SB[k] = TB[k]; SL[k] = RL[k];
    vx += RL[k] * SW[3 * k]; vy += RL[k] * SW[3 * k + 1]; vz += RL[k] * SW[3 * k + 2];
  }
  sn = rn;
  return true;
}

function result(n, i, j, k, li, lj, lk) {
  rn = n; RI[0] = i; RI[1] = j; RI[2] = k; RL[0] = li; RL[1] = lj; RL[2] = lk;
  const x = li * SW[3 * i] + lj * SW[3 * j] + lk * SW[3 * k];
  const y = li * SW[3 * i + 1] + lj * SW[3 * j + 1] + lk * SW[3 * k + 1];
  const z = li * SW[3 * i + 2] + lj * SW[3 * j + 2] + lk * SW[3 * k + 2];
  return x * x + y * y + z * z;
}

function segment(i, j) {
  const ax = SW[3 * i], ay = SW[3 * i + 1], az = SW[3 * i + 2];
  const ex = SW[3 * j] - ax, ey = SW[3 * j + 1] - ay, ez = SW[3 * j + 2] - az, ee = ex * ex + ey * ey + ez * ez;
  const t = ee > 1e-20 ? -(ax * ex + ay * ey + az * ez) / ee : 1;
  return t <= 0 ? result(1, i, 0, 0, 1, 0, 0) : t >= 1 ? result(1, j, 0, 0, 1, 0, 0) : result(2, i, j, 0, 1 - t, t, 0);
}

function triangle(i, j, k) {
  const ax = SW[3 * i], ay = SW[3 * i + 1], az = SW[3 * i + 2];
  const bx = SW[3 * j], by = SW[3 * j + 1], bz = SW[3 * j + 2];
  const cx = SW[3 * k], cy = SW[3 * k + 1], cz = SW[3 * k + 2];
  const abx = bx - ax, aby = by - ay, abz = bz - az, acx = cx - ax, acy = cy - ay, acz = cz - az;
  const d1 = -(abx * ax + aby * ay + abz * az), d2 = -(acx * ax + acy * ay + acz * az);
  if (d1 <= 0 && d2 <= 0) return result(1, i, 0, 0, 1, 0, 0);
  const d3 = -(abx * bx + aby * by + abz * bz), d4 = -(acx * bx + acy * by + acz * bz);
  if (d3 >= 0 && d4 <= d3) return result(1, j, 0, 0, 1, 0, 0);
  const vc = d1 * d4 - d3 * d2;
  if (vc <= 0 && d1 >= 0 && d3 <= 0) { const v = d1 / (d1 - d3); return result(2, i, j, 0, 1 - v, v, 0); }
  const d5 = -(abx * cx + aby * cy + abz * cz), d6 = -(acx * cx + acy * cy + acz * cz);
  if (d6 >= 0 && d5 <= d6) return result(1, k, 0, 0, 1, 0, 0);
  const vb = d5 * d2 - d1 * d6;
  if (vb <= 0 && d2 >= 0 && d6 <= 0) { const w = d2 / (d2 - d6); return result(2, i, k, 0, 1 - w, w, 0); }
  const va = d3 * d6 - d5 * d4;
  if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) { const w = (d4 - d3) / ((d4 - d3) + (d5 - d6)); return result(2, j, k, 0, 1 - w, w, 0); }
  const den = 1 / (va + vb + vc), v = vb * den, w = vc * den;
  return result(3, i, j, k, 1 - v - w, v, w);
}

// Is the origin on the far side of plane (i, j, k) from vertex l? Degenerate tetrahedra count as outside.
function outside(i, j, k, l) {
  const ax = SW[3 * i], ay = SW[3 * i + 1], az = SW[3 * i + 2];
  const ux = SW[3 * j] - ax, uy = SW[3 * j + 1] - ay, uz = SW[3 * j + 2] - az;
  const wx = SW[3 * k] - ax, wy = SW[3 * k + 1] - ay, wz = SW[3 * k + 2] - az;
  const nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
  const sp = -(ax * nx + ay * ny + az * nz), sd = (SW[3 * l] - ax) * nx + (SW[3 * l + 1] - ay) * ny + (SW[3 * l + 2] - az) * nz;
  return sp * sd <= 0;
}

const TET = [[0, 1, 2, 3], [0, 2, 3, 1], [0, 3, 1, 2], [1, 3, 2, 0]];
function tetrahedron() {
  let least = Infinity, bn = 0;
  for (const [i, j, k, l] of TET) {
    if (!outside(i, j, k, l)) continue;
    const d = triangle(i, j, k);
    if (d < least) { least = d; bn = rn; BI.set(RI); BL.set(RL); }
  }
  if (least === Infinity) return false;
  rn = bn; RI.set(BI); RL.set(BL);
  return true;
}
