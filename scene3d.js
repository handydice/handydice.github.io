import { buildDie, readValue } from './dice3d.js';
import { makeBody, mat3, randomQuat, orientValue, step } from './physics3d.js';
import { DiceRenderer } from './renderer3d.js';
import { randomFaceValues, roll } from './dice.js';

const shapes = new Map();
const shapeOf = sides => {
  if (!shapes.has(sides)) shapes.set(sides, buildDie(sides));
  return shapes.get(sides);
};
const randomBuffer = new Uint32Array(128);
let randomIndex = randomBuffer.length;
const random = () => {
  if (randomIndex === randomBuffer.length) { crypto.getRandomValues(randomBuffer); randomIndex = 0; }
  return randomBuffer[randomIndex++] / 2 ** 32;
};
const displayedValue = body => {
  const value = readValue(body.shape, mat3(body.quat, body.m));
  return body.faceValues ? body.faceValues[value - 1] : value;
};
const restoreValue = (body, value) => orientValue(body, body.faceValues ? body.faceValues.indexOf(value) + 1 : value);
const clamp = (x, lo, hi) => Math.min(hi, Math.max(lo, x));
// Tune on real phones: gentle translation in world units/s; fast tumbling in radians/s.
const MOVE_SPEED = 2, FLICK_SPEED = 20, MAX_FLICK_SPIN = 18, RANDOMIZE_SPIN = 16;
const TILT_DEADBAND = 0.01, MIN_TILT_NORMAL = Math.cos(75 * Math.PI / 180);
const renumber = body => {
  body.faceValues = randomFaceValues(body.shape.sides);
  body.faceValuesKey = body.faceValues.join(',');
};

// One WebGL context for the table and both trays. Simulation/rendering stop completely at rest.
export class DiceScene {
  constructor(canvas, table) {
    this.table = table;
    this.topTray = table.parentElement.querySelector('[data-tray="t1"]');
    this.bottomTray = table.parentElement.querySelector('[data-tray="b1"]');
    this.renderer = new DiceRenderer(canvas);
    this.renderer.onChange = () => this.invalidate();
    this.bodies = new Map();
    this.targets = new Map();
    this.active = [];
    this.tableBodies = [];
    this.previews = [];
    this.bounds = { x: 3, z: 4 };
    this.gravity = [0, -1, 0];
    this.raf = 0;
    this.lastTime = 0;
    this.accumulator = 0;
    this.grip = null;
    this.throwing = null;
    this.onSettle = null;
    this.onError = null;
    this.onFlickRandomize = null;
    this.onImpact = null;
    this.rect = table.getBoundingClientRect();
    this.frame = time => this.tick(time);
    this.lost = () => {
      this.failed = true;
      cancelAnimationFrame(this.raf); this.raf = 0;
      if (this.grip) this.cancelGrip();
      // Losing the renderer aborts the throw; restore the last committed round before recovery.
      for (const body of this.bodies.values()) {
        restoreValue(body, body.die.value);
        body.vel.fill(0); body.ang.fill(0); body.sleeping = true;
      }
      this.dropped = false;
      this.throwing?.resolve(); this.throwing = null;
    };
    this.restored = () => { this.failed = false; this.invalidate(); };
    this.visibility = () => {
      if (document.hidden) { cancelAnimationFrame(this.raf); this.raf = 0; this.lastTime = 0; }
      else this.invalidate();
    };
    canvas.addEventListener('webglcontextlost', this.lost);
    canvas.addEventListener('webglcontextrestored', this.restored);
    document.addEventListener('visibilitychange', this.visibility);
  }

  sync(dice, targets, surface, theme) {
    for (const [d] of this.bodies) if (!dice.includes(d)) this.bodies.delete(d);
    const count = dice.filter(d => !d.tray).length;
    const columns = Math.max(1, Math.ceil(Math.sqrt(count * this.rect.width / Math.max(1, this.rect.height))));
    let index = 0;
    for (const d of dice) {
      let body = this.bodies.get(d);
      if (!body || body.shape.sides !== d.sides) {
        const x = ((index % columns) - (columns - 1) / 2) * 1.4;
        const z = (Math.floor(index / columns) - (Math.ceil(count / columns) - 1) / 2) * 1.4;
        body = makeBody(shapeOf(d.sides), [x, 1, z]);
        restoreValue(body, d.value);
        body.sleeping = true;
        body.die = d;
        this.bodies.set(d, body);
      }
      body.color = d.color || 'white';
      body.numbered = !!d.numbered;
      if (body.tray !== d.tray) {
        if (!d.tray) {
          this.placeOnTable(body);
        }
        body.tray = d.tray;
        restoreValue(body, d.value);
        body.vel.fill(0); body.ang.fill(0); body.sleeping = true;
      }
      if (!d.tray) index++;
    }
    this.targets = targets;
    this.renderer.setSurface(surface, theme);
    this.layout();
  }

  placeOnTable(body) {
    const xMax = this.bounds.x - body.shape.radius - 0.1;
    const zMin = (this.bounds.minZ ?? -this.bounds.z) + body.shape.radius + 0.1;
    const zMax = (this.bounds.maxZ ?? this.bounds.z) - body.shape.radius - 0.1;
    let best = -Infinity, nearest = Infinity;
    // ponytail: 25 candidates, at most 12 dice; use packing if the dice limit grows.
    for (let iz = -2; iz <= 2; iz++) for (let ix = -2; ix <= 2; ix++) {
      const x = ix * xMax / 2, z = (zMin + zMax) / 2 + iz * (zMax - zMin) / 4;
      let clearance = Infinity;
      for (const other of this.bodies.values()) {
        if (other === body || other.tray || other.die.tray) continue;
        clearance = Math.min(clearance, Math.hypot(x - other.pos[0], z - other.pos[2]) - body.shape.radius - other.shape.radius);
      }
      const distance = x * x + z * z;
      if (clearance > best || clearance === best && distance < nearest) {
        best = clearance; nearest = distance;
        body.pos[0] = x; body.pos[2] = z;
      }
    }
  }

  layout() {
    const old = this.bounds, oldRect = this.rect;
    this.rect = this.table.getBoundingClientRect();
    this.renderer.measure();
    const ratio = Math.max(0.4, this.rect.width / Math.max(1, this.rect.height));
    // Large screens add rolling space instead of magnifying the dice.
    const half = 2.7 * Math.max(1, Math.min(this.rect.width, this.rect.height) / 512);
    this.bounds = { x: half * Math.max(1, ratio), z: half * Math.max(1, 1 / ratio) };
    this.playTop = this.topTray.classList.contains('occupied') ? this.rect.top : this.topTray.getBoundingClientRect().top;
    this.playBottom = this.bottomTray.classList.contains('occupied') ? this.rect.top + this.rect.height : this.bottomTray.getBoundingClientRect().bottom;
    this.bounds.minZ = this.playTop === this.rect.top ? -this.bounds.z
      : this.renderer.unproject(this.rect.left, this.playTop, this.rect, this.bounds, 0)[2];
    this.bounds.maxZ = this.playBottom === this.rect.top + this.rect.height ? this.bounds.z
      : this.renderer.unproject(this.rect.left, this.playBottom, this.rect, this.bounds, 0)[2];
    const resize = oldRect.width !== this.rect.width || oldRect.height !== this.rect.height;
    const shrinkX = this.bounds.x < old.x;
    const shrinkZ = this.bounds.minZ > (old.minZ ?? -old.z) || this.bounds.maxZ < (old.maxZ ?? old.z);
    for (const body of this.bodies.values()) {
      if (body.die.tray) continue;
      if (resize) {
        body.pos[0] *= this.bounds.x / old.x;
        body.pos[2] *= this.bounds.z / old.z;
      }
      if (!shrinkX && !shrinkZ) continue;
      // Only a shrinking wall can move a die. Use its actual rotated shape, not its bounding sphere.
      const m = mat3(body.quat, body.m), v = body.shape.vertices, margin = body.shape.margin;
      let minX = Infinity, maxX = -Infinity, minZ = Infinity, maxZ = -Infinity;
      for (let i = 0; i < v.length; i += 3) {
        const x = m[0] * v[i] + m[3] * v[i + 1] + m[6] * v[i + 2];
        const z = m[2] * v[i] + m[5] * v[i + 1] + m[8] * v[i + 2];
        minX = Math.min(minX, x); maxX = Math.max(maxX, x);
        minZ = Math.min(minZ, z); maxZ = Math.max(maxZ, z);
      }
      if (shrinkX) body.pos[0] = clamp(body.pos[0], -this.bounds.x - minX + margin, this.bounds.x - maxX - margin);
      if (shrinkZ) body.pos[2] = clamp(body.pos[2], this.bounds.minZ - minZ + margin, this.bounds.maxZ - maxZ - margin);
    }
    this.refreshEntries();
    this.placeTargets();
    this.invalidate();
  }

  refreshEntries() {
    this.tableBodies.length = 0;
    this.active.length = 0;
    this.previews.length = 0;
    for (const [d, body] of this.bodies) {
      const el = this.targets.get(d);
      body.highlight = !!el?.classList.contains('selected');
      if (!d.tray && body !== this.grip?.body) {
        this.tableBodies.push(body);
        this.active.push(body);
      } else if (body !== this.grip?.body && el) {
        const r = el.getBoundingClientRect(), container = el.parentElement;
        const clip = container.getBoundingClientRect();
        if (r.bottom > clip.top && r.top < clip.bottom && r.right > clip.left && r.left < clip.right) {
          const scrolling = container.scrollHeight > container.clientHeight || container.scrollWidth > container.clientWidth;
          this.previews.push({ body, rect: r, clip: scrolling ? clip : undefined });
        }
      }
    }
    if (this.grip) {
      const { body, x, y } = this.grip;
      this.active.push(body);
      if (x >= this.rect.left && x <= this.rect.right && y >= this.playTop && y <= this.playBottom) this.tableBodies.push(body);
      else this.previews.push({ body, rect: { left: x - 42, top: y - 42, width: 84, height: 84, right: x + 42, bottom: y + 42 } });
    }
  }

  invalidate() {
    if (!this.failed && !this.raf && !document.hidden) this.raf = requestAnimationFrame(this.frame);
  }

  tick(time) {
    this.raf = 0;
    const dt = this.lastTime ? Math.min(0.05, (time - this.lastTime) / 1000) : 1 / 60;
    this.lastTime = time;
    if (this.grip && time - this.grip.lastMotion > 80) this.grip.body.held.ang.fill(0);
    if (this.grip && !this.grip.rotated) {
      const { body } = this.grip, q = body.quat, target = body.held.quat;
      const blend = 1 - Math.exp(-12 * dt);
      for (let i = 0; i < 4; i++) q[i] += (target[i] - q[i]) * blend;
      const length = Math.hypot(...q);
      for (let i = 0; i < 4; i++) q[i] /= length;
    }
    // Sample release spin before contacts dissipate it.
    for (const body of this.active) {
      if (!body.randomizeOnSpin) continue;
      if (Math.hypot(body.ang[0], body.ang[2]) >= RANDOMIZE_SPIN) {
        renumber(body);
        body.randomizeOnSpin = false; // once per throw; yaw alone leaves the upper face readable
        this.onFlickRandomize?.(body.die, true);
      } else if (body.sleeping) body.randomizeOnSpin = false;
    }
    let moving = this.active.some(body => !body.sleeping || body.held);
    if (moving) {
      this.accumulator += dt;
      while (this.accumulator >= 1 / 120) {
        step(this.active, 1 / 120, this.bounds, this.gravity, this.onImpact);
        this.accumulator -= 1 / 120;
      }
      moving = this.active.some(body => !body.sleeping || body.held);
    }
    this.draw();
    if (!moving && this.throwing) {
      const { dice, resolve } = this.throwing;
      this.throwing = null;
      for (const d of dice) d.value = displayedValue(this.bodies.get(d));
      resolve();
    } else if (!moving && this.dropped) {
      this.dropped = false;
      for (const body of this.tableBodies) body.die.value = displayedValue(body);
      this.onSettle?.();
    }
    if (moving) this.invalidate();
    else { this.lastTime = 0; this.accumulator = 0; }
  }

  draw() {
    try {
      this.renderer.render(this.tableBodies, this.rect, this.bounds, this.previews);
      this.placeTargets();
    } catch (error) {
      this.failed = true;
      this.throwing?.resolve(); this.throwing = null;
      this.onError?.(error);
    }
  }

  placeTargets() {
    for (const body of this.tableBodies) {
      const el = this.targets.get(body.die);
      if (!el || body === this.grip?.body && body.die.tray) continue;
      const { x, y, size } = this.renderer.project(body, this.rect, this.bounds);
      const width = Math.max(44, size);
      el.style.left = `${x - this.rect.left - width / 2}px`;
      el.style.top = `${y - this.rect.top - width / 2}px`;
      el.style.width = `${width}px`;
      el.style.height = `${width}px`;
      el.style.zIndex = String(Math.round(body.pos[1] * 10 + 10));
    }
  }
  pick(x, y) { return this.renderer.raycast(x, y, this.tableBodies, this.rect, this.bounds); }

  setTilt(beta, gamma, screenAngle = 0) {
    if (!Number.isFinite(beta) || !Number.isFinite(gamma) || !Number.isFinite(screenAngle)) return;
    const b = beta * Math.PI / 180, g = gamma * Math.PI / 180, a = screenAngle * Math.PI / 180;
    let x = Math.sin(g) * Math.cos(b), z = Math.sin(b);
    // ponytail: cap tilt at 75°; off-table falls would need a catch/return model.
    let normal = Math.cos(b) * Math.cos(g);
    const horizontal = Math.hypot(x, z);
    if (normal < MIN_TILT_NORMAL) {
      if (horizontal < 1e-9) { x = z = 0; normal = 1; }
      else {
        const scale = Math.sqrt(1 - MIN_TILT_NORMAL ** 2) / horizontal;
        x *= scale; z *= scale; normal = MIN_TILT_NORMAL;
      }
    }
    const length = Math.hypot(x, normal, z);
    x /= length; z /= length;
    const gx = x * Math.cos(a) + z * Math.sin(a), gy = -normal / length;
    const gz = z * Math.cos(a) - x * Math.sin(a);
    if (Math.hypot(gx - this.gravity[0], gy - this.gravity[1], gz - this.gravity[2]) < TILT_DEADBAND) return;
    this.gravity[0] = gx; this.gravity[1] = gy; this.gravity[2] = gz;
    for (const body of this.bodies.values()) {
      if (body.die.tray || body.held) continue;
      body.sleeping = false; body.sleepTimer = 0; body.guided = false;
      body.randomizeOnSpin = false; // sensor play must never trigger cryptographic rerolls
    }
    this.dropped = true;
    this.invalidate();
  }

  throw(dice, { calm } = {}) {
    this.dropped = false;
    const n = dice.length, columns = Math.ceil(Math.sqrt(n * this.rect.width / Math.max(1, this.rect.height)));
    dice.forEach((d, i) => {
      const b = this.bodies.get(d);
      // Independent numbering makes every physical face uniformly random, including unequal d100 faces.
      renumber(b);
      b.guided = false; b.randomizeOnSpin = false;
      if (calm) {
        d.value = roll(d.sides);
        restoreValue(b, d.value);
        return;
      }
      b.quat = randomQuat(random);
      b.pos[0] = ((i % columns) - (columns - 1) / 2) * 1.25;
      b.pos[1] = 2.5 + Math.floor(i / columns) * 1.2 + random() * 0.4;
      b.pos[2] = (Math.floor(i / columns) - (Math.ceil(n / columns) - 1) / 2) * 1.25;
      b.vel[0] = (random() - 0.5) * 4;
      b.vel[1] = 2 + random() * 3;
      b.vel[2] = (random() - 0.5) * 4;
      b.ang[0] = (random() - 0.5) * 24;
      b.ang[1] = (random() - 0.5) * 24;
      b.ang[2] = (random() - 0.5) * 24;
      b.sleeping = false; b.sleepTimer = 0;
    });
    if (calm) { this.invalidate(); return Promise.resolve(); }
    this.refreshEntries();
    this.invalidate();
    return new Promise(resolve => { this.throwing = { dice, resolve }; });
  }

  beginGrip(d, x, y, time = performance.now()) {
    const body = this.bodies.get(d);
    if (!body) return;
    // Stay inside the current face's angular region, including tiny D100 faces.
    const m = mat3(body.quat, body.m), faceValue = readValue(body.shape, m);
    const n = body.shape.faces.find(face => face.value === faceValue).normal;
    const sign = body.shape.bottomRead ? -1 : 1;
    let margin = 1;
    for (const face of body.shape.faces) {
      const f = face.normal;
      if (f === n) continue;
      const x = n[0] - f[0], y = n[1] - f[1], z = n[2] - f[2];
      margin = Math.min(margin, sign * (m[1] * x + m[4] * y + m[7] * z) / Math.hypot(x, y, z));
    }
    this.grip = {
      body, x, y, originalPos: [...body.pos], originalQuat: [...body.quat],
      originalVel: [...body.vel], originalAng: [...body.ang], originalSleeping: body.sleeping,
      originalTray: body.tray, originalGuided: body.guided, originalRandomize: body.randomizeOnSpin,
      vx: 0, vz: 0, lastMotion: performance.now(),
      wobbleLimit: Math.min(0.08, Math.asin(clamp(margin, 0, 1)) / 2),
    };
    body.sleeping = false; body.sleepTimer = 0; body.guided = false; body.randomizeOnSpin = false;
    body.held = { target: [...body.pos], ang: [0, 0, 0], quat: [...body.quat] };
    this.dropped = true; // commit every displaced die after pickup, including neighboring dice
    this.moveGrip(x, y, 0, 0);
    this.grip.lastMotion = time;
  }

  moveGrip(x, y, dx, dy, rotate = false) {
    if (!this.grip) return;
    const { body } = this.grip;
    const now = performance.now(), dt = Math.max((now - this.grip.lastMotion) / 1000, 1 / 120);
    this.grip.lastMotion = now;
    if (!rotate) { this.grip.x = x; this.grip.y = y; }
    if (!rotate) {
      const target = this.renderer.unproject(x, y, this.rect, this.bounds, 1.6);
      target[0] = clamp(target[0], -this.bounds.x + body.shape.radius, this.bounds.x - body.shape.radius);
      target[2] = clamp(target[2], (this.bounds.minZ ?? -this.bounds.z) + body.shape.radius, (this.bounds.maxZ ?? this.bounds.z) - body.shape.radius);
      this.grip.vx = dx || dy ? (target[0] - body.held.target[0]) / dt : 0;
      this.grip.vz = dx || dy ? (target[2] - body.held.target[2]) / dt : 0;
      body.held.target = target;
      if (!this.grip.rotated) {
        const speed = Math.hypot(this.grip.vx, this.grip.vz);
        const angle = this.grip.wobbleLimit * speed / (speed + MOVE_SPEED);
        const scale = speed ? Math.sin(angle / 2) / speed : 0, c = Math.cos(angle / 2);
        const x = this.grip.vz * scale, z = -this.grip.vx * scale;
        const [bx, by, bz, bw] = this.grip.originalQuat, q = body.held.quat;
        q[0] = c * bx + x * bw - z * by;
        q[1] = c * by + z * bx - x * bz;
        q[2] = c * bz + z * bw + x * by;
        q[3] = c * bw - x * bx - z * bz;
      }
    }
    if (rotate) {
      this.grip.vx = this.grip.vz = 0;
      if (dx || dy) this.grip.rotated = true;
      body.held.ang[0] = clamp(dy * 0.3, -24, 24);
      body.held.ang[1] = clamp(dx * 0.3, -24, 24);
    } else body.held.ang.fill(0);
    this.refreshEntries(); this.invalidate();
  }

  endGrip(d, cancelled = false, autoPlace = false) {
    if (!this.grip) return;
    const { body, originalPos, originalQuat, originalVel, originalAng, originalSleeping, originalGuided, originalRandomize } = this.grip;
    const heldAng = body.held?.ang;
    body.held = null;
    body.tray = d.tray;
    if (cancelled) {
      body.pos = originalPos; body.quat = originalQuat;
      body.vel = originalVel; body.ang = originalAng; body.sleeping = originalSleeping;
      body.guided = originalGuided; body.randomizeOnSpin = originalRandomize;
    } else if (autoPlace && this.grip.originalTray && !d.tray) {
      this.placeOnTable(body);
      restoreValue(body, d.value);
      body.vel.fill(0); body.ang.fill(0); body.sleeping = true;
    } else if (d.tray) {
      // A tray is storage, not another roll: keep the result from before pickup.
      restoreValue(body, d.value);
      body.vel.fill(0); body.ang.fill(0); body.sleeping = true;
    } else {
      const stale = performance.now() - this.grip.lastMotion > 80;
      const vx = stale ? 0 : this.grip.vx, vz = stale ? 0 : this.grip.vz;
      const speed = Math.hypot(vx, vz), t = clamp((speed - MOVE_SPEED) / (FLICK_SPEED - MOVE_SPEED), 0, 1);
      const blend = t * t * (3 - 2 * t); // continuous speed and spin, including at the threshold
      body.vel[0] = clamp(vx, -8, 8) * blend; body.vel[2] = clamp(vz, -8, 8) * blend;
      body.vel[1] = Math.min(body.vel[1], 0); // hand lift is not an upward throw impulse
      body.guided = blend === 0 && !this.grip.rotated;
      body.randomizeOnSpin = !body.guided;
      if (body.guided) {
        // Lower naturally, without changing the face or adding sideways momentum.
        body.vel[0] = body.vel[2] = 0; body.ang.fill(0);
      } else if (!this.grip.rotated) {
        const spin = MAX_FLICK_SPIN * blend / speed;
        body.ang[0] = vz * spin;
        body.ang[1] = 0;
        body.ang[2] = -vx * spin;
      } else if (heldAng) {
        body.ang[0] = heldAng[0]; body.ang[1] = heldAng[1]; body.ang[2] = heldAng[2];
      }
      body.sleeping = false; body.sleepTimer = 0;
      this.dropped = true;
      this.onFlickRandomize?.(d, false);
    }
    this.grip = null;
    this.refreshEntries(); this.invalidate();
  }

  cancelGrip() { if (this.grip) this.endGrip(this.grip.body.die, true); }

  dispose() {
    this.failed = true;
    cancelAnimationFrame(this.raf); this.raf = 0;
    const canvas = this.renderer.canvas;
    canvas.removeEventListener('webglcontextlost', this.lost);
    canvas.removeEventListener('webglcontextrestored', this.restored);
    document.removeEventListener('visibilitychange', this.visibility);
    this.throwing?.resolve(); this.throwing = null;
    this.grip = null; this.onSettle = null; this.onError = null; this.onFlickRandomize = null;
    this.renderer.dispose();
  }
}
