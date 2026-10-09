import assert from 'node:assert/strict';
import { ATLAS_COLS, buildDie, readValue } from './dice3d.js';

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

// Column-major rotation turning unit vector n onto unit vector t (Rodrigues).
function rotateOnto(n, t) {
  const k = cross(n, t), s = Math.hypot(...k), c = dot(n, t);
  if (s < 1e-12) return c > 0 ? [1, 0, 0, 0, 1, 0, 0, 0, 1] : [1, 0, 0, 0, -1, 0, 0, 0, -1];
  const [x, y, z] = k.map(v => v / s), C = 1 - c;
  const r = [ // row-major
    [c + x * x * C, x * y * C - z * s, x * z * C + y * s],
    [y * x * C + z * s, c + y * y * C, y * z * C - x * s],
    [z * x * C - y * s, z * y * C + x * s, c + z * z * C]];
  return [0, 1, 2].flatMap(col => [0, 1, 2].map(row => r[row][col]));
}

// Solid angle of a planar polygon seen from the center (Van Oosterom–Strackee, fanned).
function solidAngle(poly) {
  let sum = 0;
  for (let i = 1; i < poly.length - 1; i++) {
    const [a, b, c] = [poly[0], poly[i], poly[i + 1]], [la, lb, lc] = [a, b, c].map(v => Math.hypot(...v));
    sum += 2 * Math.atan2(Math.abs(dot(a, cross(b, c))), la * lb * lc + dot(a, b) * lc + dot(a, c) * lb + dot(b, c) * la);
  }
  return sum;
}

for (const sides of [4, 6, 8, 10, 12, 20, 100]) {
  const die = buildDie(sides), { faces } = die;
  assert.equal(buildDie(sides), die, `d${sides} is cached`);
  assert.equal(faces.length, sides);
  assert.deepEqual(faces.map(f => f.value).sort((a, b) => a - b), Array.from({ length: sides }, (_, i) => i + 1), `d${sides} numbers 1..${sides} once`);
  assert.equal(die.rows, Math.ceil((sides + 1) / ATLAS_COLS));
  assert.ok(die.margin > 0 && die.inradius > die.margin && die.radius > die.inradius && die.radius < 0.9, `d${sides} sane sizes`);

  // Faces: planar polygons at the rest height, convex hull of the core contains every vertex.
  const core = [];
  for (let i = 0; i < die.vertices.length; i += 3) core.push([die.vertices[i], die.vertices[i + 1], die.vertices[i + 2]]);
  for (const f of faces) {
    assert.ok(f.poly.length >= 3 && f.poly.length === f.idx.length && f.poly.length === f.flat.length);
    for (const p of f.poly) assert.ok(Math.abs(dot(p, f.normal) - die.inradius) < 1e-6, `d${sides} face ${f.value} is flat`);
    for (const p of core) assert.ok(dot(p, f.normal) <= die.inradius - die.margin + 1e-6, `d${sides} core is convex`);
    for (const [x, y] of f.flat) assert.ok(x >= 0 && x <= 1 && y >= 0 && y <= 1, `d${sides} face inside its cell`);
    assert.ok(f.labels.every(l => l.x > 0 && l.x < 1 && l.y > 0 && l.y < 1 && l.size > 0.05 && Math.abs(Math.hypot(...l.up) - 1) < 1e-9));
  }

  // Opposite faces sum to sides+1 (the d4 has none).
  if (sides !== 4) for (const f of faces) {
    const opposite = faces.find(g => dot(f.normal, g.normal) < -0.999999);
    assert.equal(f.value + opposite.value, sides + 1, `d${sides}: ${f.value} opposite ${opposite.value}`);
  }

  // Mesh: closed (every edge shared by exactly two triangles in opposite directions) and facing outward.
  const { positions: P, normals: N, indices: I, uvs } = die;
  assert.ok(I.length % 3 === 0 && Math.max(...I) < P.length / 3);
  const at = i => [P[3 * i], P[3 * i + 1], P[3 * i + 2]], key = i => at(i).join();
  const directed = new Map();
  let volume = 0;
  for (let t = 0; t < I.length; t += 3) {
    const tri = [I[t], I[t + 1], I[t + 2]], [a, b, c] = tri.map(at);
    for (let k = 0; k < 3; k++) {
      const u = key(tri[k]), v = key(tri[(k + 1) % 3]);
      if (u !== v) directed.set(`${u}>${v}`, (directed.get(`${u}>${v}`) ?? 0) + 1);
    }
    volume += dot(a, cross(b, c)) / 6;
    const n = cross(sub(b, a), sub(c, a));
    if (Math.hypot(...n) > 1e-7) {
      const vn = tri.reduce((s, i) => [s[0] + N[3 * i], s[1] + N[3 * i + 1], s[2] + N[3 * i + 2]], [0, 0, 0]);
      assert.ok(dot(n, vn) > 0, `d${sides}: triangle ${t / 3} winds outward`);
    }
  }
  for (const [edge, count] of directed) {
    const [u, v] = edge.split('>');
    assert.equal(count, 1, `d${sides}: edge used once per direction`);
    assert.equal(directed.get(`${v}>${u}`), 1, `d${sides}: mesh is closed`);
  }
  assert.ok(volume > 0, `d${sides} has positive volume`);
  for (let i = 0; i < N.length; i += 3) assert.ok(Math.abs(Math.hypot(N[i], N[i + 1], N[i + 2]) - 1) < 1e-6);
  for (const c of uvs) assert.ok(c >= 0 && c <= 1, `d${sides} uvs inside the atlas`);

  // Turning any face up reads its value; a d4 is read at the apex, i.e. from the face it rests on.
  for (const f of faces) assert.equal(readValue(die, rotateOnto(f.normal, [0, die.bottomRead ? -1 : 1, 0])), f.value);
}

// d4: each face shows the three values at its corners, each corner the value of the face opposite it, so
// the apex of a resting d4 shows the rolled value on all three visible faces.
{
  const d4 = buildDie(4);
  assert.equal(d4.bottomRead, true);
  for (const f of d4.faces) {
    assert.equal(f.labels.length, 3);
    for (const label of f.labels) {
      const corner = f.flat.reduce((best, p, k) => Math.hypot(p[0] - label.x, p[1] - label.y) < Math.hypot(f.flat[best][0] - label.x, f.flat[best][1] - label.y) ? k : best, 0);
      const opposite = d4.faces.find(g => !g.idx.includes(f.idx[corner]));
      assert.equal(label.value, opposite.value, 'apex numbering');
      const toward = [f.flat[corner][0] - label.x, f.flat[corner][1] - label.y];
      assert.ok(dot([...label.up, 0], [...toward, 0]) > 0.99 * Math.hypot(...toward), 'corner labels stand toward their corner');
    }
  }
}

// Other dice print their own value once, upright in the cell.
for (const f of buildDie(20).faces) {
  assert.deepEqual(f.labels.map(l => l.value), [f.value]);
  assert.ok(Math.abs(f.labels[0].up[1] - 1) < 1e-12);
}

// d6: 1, 2, 3 run counterclockwise around their shared corner (Western dice); faces are squares upright in their cells.
{
  const d6 = buildDie(6), n = v => d6.faces.find(f => f.value === v).normal;
  assert.ok(dot(n(1), cross(n(2), n(3))) > 0.99);
  for (const f of d6.faces) for (const [x, y] of f.flat) assert.ok(Math.min(Math.abs(x - 0.02), Math.abs(x - 0.98)) < 1e-9 && Math.min(Math.abs(y - 0.02), Math.abs(y - 0.98)) < 1e-9);
}

// d10: congruent kites, odd numbers around one pole and even around the other, taller than wide.
{
  const d10 = buildDie(10);
  for (const f of d10.faces) {
    assert.equal(f.poly.length, 4);
    assert.equal(Math.sign(f.normal[1]), f.value % 2 ? 1 : -1);
  }
  const ys = [], rs = [];
  for (let i = 0; i < d10.vertices.length; i += 3) { ys.push(d10.vertices[i + 1]); rs.push(Math.hypot(d10.vertices[i], d10.vertices[i + 2])); }
  const ratio = Math.max(...ys) / Math.max(...rs);
  assert.ok(ratio > 1 && ratio < 1.3, `d10 proportions (${ratio.toFixed(2)})`);
}

// d100: one hundred real faces in opposite pairs, nearly equal in size, with room for three digits.
{
  const d100 = buildDie(100), v = d100.vertices;
  const angles = d100.faces.map(f => solidAngle(f.idx.map(k => [v[3 * k], v[3 * k + 1], v[3 * k + 2]])));
  assert.ok(Math.abs(angles.reduce((a, b) => a + b) - 4 * Math.PI) < 1e-4, 'core faces tile the sphere of directions');
  const spread = Math.max(...angles) / Math.min(...angles);
  console.log(`d100 face solid angle max/min ${spread.toFixed(3)}`);
  assert.ok(spread < 1.3, 'd100 faces nearly alike');
  assert.ok(d100.faces.every(f => f.poly.length >= 4 && f.labels[0].size > 0.2), 'd100 labels have room');
}

console.log('dice3d ok');
