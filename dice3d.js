// dice3d.js — geometry of physical dice, pure logic. Every die is the solid {x : n·x ≤ 1} of its unit face
// normals: all faces lie at the same distance from the center, so faces are planar by construction and the
// platonic dice and the d10 are isohedral (every face alike). The solid is shrunk to a core and rounded by
// a ball of radius `margin` (core ⊕ ball), like the bevelled edges and corners of real plastic dice. The
// physics collides exactly that shape, so rendered dice neither float nor sink.
export const ATLAS_COLS = 8;

const PHI = (1 + Math.sqrt(5)) / 2;
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (v, s) => [v[0] * s, v[1] * s, v[2] * s];
const norm = v => scale(v, 1 / Math.hypot(...v));

// ---- convex solid from face normals ----
function clip(poly, m) {
  const out = [];
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i], q = poly[(i + 1) % poly.length], dp = dot(p, m) - 1, dq = dot(q, m) - 1;
    if (dp <= 0) out.push(p);
    if (dp * dq < 0) out.push(add(p, scale(sub(q, p), dp / (dp - dq))));
  }
  return out;
}

// Face polygons, CCW seen from outside: a big square in each face plane clipped by all other planes.
function facePolys(normals) {
  return normals.map(n => {
    const t = norm(cross(Math.abs(n[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0], n)), b = cross(n, t);
    let poly = [[1, 1], [-1, 1], [-1, -1], [1, -1]].map(([u, v]) => add(n, add(scale(t, 4 * u), scale(b, 4 * v))));
    for (const m of normals) if (m !== n) poly = clip(poly, m);
    return poly;
  });
}

// Shared vertices: each corner is computed once per face that meets there.
function weld(polys) {
  const verts = [];
  const rings = polys.map(poly => {
    const idx = [];
    for (const p of poly) {
      let i = verts.findIndex(v => Math.abs(v[0] - p[0]) + Math.abs(v[1] - p[1]) + Math.abs(v[2] - p[2]) < 1e-7);
      if (i < 0) i = verts.push(p) - 1;
      if (idx[idx.length - 1] !== i) idx.push(i);
    }
    if (idx.length > 1 && idx[0] === idx[idx.length - 1]) idx.pop();
    return idx;
  });
  return { verts, rings };
}

function centroid(poly) {
  let c = [0, 0, 0], area = 0;
  for (let i = 1; i < poly.length - 1; i++) {
    const a = Math.hypot(...cross(sub(poly[i], poly[0]), sub(poly[i + 1], poly[0])));
    area += a;
    c = add(c, scale(add(add(poly[0], poly[i]), poly[i + 1]), a / 3));
  }
  return scale(c, 1 / area);
}

// ---- the dice: face normals and numbering ----
// Opposite faces sum to sides+1; preset values stay, their opposites follow.
function number(normals, sides, values = []) {
  let next = 1;
  normals.forEach((n, i) => {
    if (values[i]) return;
    const j = normals.findIndex(m => dot(m, n) < -0.999999);
    if (values[j]) values[i] = sides + 1 - values[j];
    else { while (values.includes(next)) next++; values[i] = next; values[j] = sides + 1 - next; }
  });
  return values;
}

const corners = [];
for (const x of [1, -1]) for (const y of [1, -1]) for (const z of [1, -1]) corners.push([x, y, z]);
const cyclic = (a, b) => [[0, a, b], [a, b, 0], [b, 0, a]];
const signed = (a, b) => [[a, b], [a, -b], [-a, b], [-a, -b]];

// Pentagonal trapezohedron: kite normals tilted θ from the poles, the lower ring turned by 36°. All ten
// faces are congruent planar kites; pole-to-pole height / belt width = tan θ / 1.1056 ≈ 1.1, like real d10s.
function d10Normals() {
  const theta = Math.atan(1.1 * 2 / (1 + Math.cos(Math.PI / 5))), s = Math.sin(theta), c = Math.cos(theta);
  const out = [];
  for (const up of [1, -1]) for (let k = 0; k < 5; k++) {
    const a = (2 * k + (up < 0 ? 1 : 0)) * Math.PI / 5;
    out.push([s * Math.cos(a), up * c, s * Math.sin(a)]);
  }
  return out;
}

// d100: a convex die with 100 faces in 50 opposite pairs (like a Zocchihedron, but with flat faces so it
// rests on one face). Lloyd relaxation of a Fibonacci spiral evens out the face sizes; it is not isohedral,
// so faces are only nearly alike.
function d100Normals() {
  const ga = Math.PI * (3 - Math.sqrt(5));
  let pts = Array.from({ length: 50 }, (_, i) => {
    const y = 1 - (i + 0.5) / 50, r = Math.sqrt(1 - y * y);
    return [r * Math.cos(i * ga), y, r * Math.sin(i * ga)];
  });
  const both = () => pts.flatMap(p => [p, scale(p, -1)]);
  for (let it = 0; it < 30; it++) pts = facePolys(both()).filter((_, i) => i % 2 === 0).map(poly => norm(centroid(poly)));
  return both();
}

function solid(sides) {
  switch (sides) {
    case 4: return { normals: [[-1, -1, -1], [-1, 1, 1], [1, -1, 1], [1, 1, -1]].map(norm), values: [1, 2, 3, 4] };
    // 1, 2, 3 counterclockwise around their corner, as on Western dice.
    case 6: return { normals: [[0, 0, 1], [1, 0, 0], [0, 1, 0], [0, -1, 0], [-1, 0, 0], [0, 0, -1]], values: [1, 2, 3, 4, 5, 6] };
    case 8: return { normals: corners.map(norm) };
    // Odd numbers around the upper pole, even around the lower one.
    case 10: return { normals: d10Normals(), values: [1, 7, 3, 9, 5] };
    case 12: return { normals: signed(1, PHI).flatMap(([a, b]) => cyclic(a, b)).map(norm) };
    case 20: return { normals: [...corners, ...signed(1 / PHI, PHI).flatMap(([a, b]) => cyclic(a, b))].map(norm) };
    case 100: return { normals: d100Normals() };
    default: throw new RangeError(`no d${sides}`);
  }
}

// Sharp circumradius and bevel radius in world units (a d6 is 0.9 wide).
const SIZE = { 4: [0.68, 0.06], 6: [0.78, 0.07], 8: [0.66, 0.055], 10: [0.66, 0.05], 12: [0.68, 0.05], 20: [0.7, 0.045], 100: [0.8, 0.03] };

// Face texture layout: an orthonormal basis in the face plane with "up" toward the label's top, the face
// scaled uniformly into its atlas cell (x right, y up, 0..1), and the labels to print.
function layout(poly, n, sides, cornerValues, value) {
  const c = centroid(poly);
  const up = sides === 6 ? scale(add(poly[0], poly[1]), 0.5)
    : poly.reduce((best, p) => Math.hypot(...sub(p, c)) > Math.hypot(...sub(best, c)) + 1e-9 ? p : best);
  const bv = norm(sub(sub(up, c), scale(n, dot(sub(up, c), n)))), tv = cross(bv, n);
  const local = p => [dot(sub(p, c), tv), dot(sub(p, c), bv)];
  const pts = poly.map(local);
  const lo = [0, 1].map(k => Math.min(...pts.map(p => p[k]))), hi = [0, 1].map(k => Math.max(...pts.map(p => p[k])));
  const span = Math.max(hi[0] - lo[0], hi[1] - lo[1]) / 0.96;
  const toCell = p => [0.5 + (p[0] - (lo[0] + hi[0]) / 2) / span, 0.5 + (p[1] - (lo[1] + hi[1]) / 2) / span];
  // Radius of the free circle around a point: its distance to the nearest edge.
  const free = q => Math.min(...pts.map((p, k) => {
    const r = pts[(k + 1) % pts.length], e = [r[0] - p[0], r[1] - p[1]];
    return Math.abs(e[0] * (q[1] - p[1]) - e[1] * (q[0] - p[0])) / Math.hypot(...e);
  })) / span;
  const label = (v, at, toward) => {
    const q = local(at), d = sub(local(toward), q), l = Math.hypot(d[0], d[1]);
    const [x, y] = toCell(q);
    return { value: v, x, y, up: [d[0] / l, d[1] / l], size: free(q) };
  };
  // A d4 is read at its apex: each corner shows the value of the face opposite that corner.
  const labels = cornerValues
    ? poly.map((p, k) => label(cornerValues[k], add(c, scale(sub(p, c), 0.55)), p))
    : [label(value, c, add(c, bv))];
  return { flat: pts.map(toCell), size: [hi[0] - lo[0], hi[1] - lo[1]], labels };
}

function slerp(a, b, angle, t) {
  const s = Math.sin(angle);
  return add(scale(a, Math.sin((1 - t) * angle) / s), scale(b, Math.sin(t * angle) / s));
}

function make(sides) {
  const { normals, values: preset } = solid(sides);
  const values = sides === 4 ? preset : number(normals, sides, preset);
  const [R, margin] = SIZE[sides];
  const { verts, rings } = weld(facePolys(normals));
  const inradius = R / Math.max(...verts.map(v => Math.hypot(...v)));
  const core = verts.map(v => scale(v, inradius - margin));
  const F = normals.length, rows = Math.ceil((F + 1) / ATLAS_COLS);
  const edgeU = (F % ATLAS_COLS + 0.5) / ATLAS_COLS, edgeV = 1 - (Math.floor(F / ATLAS_COLS) + 0.5) / rows;
  const positions = [], vnormals = [], uvs = [], indices = [];
  const vertex = (p, n, u, v) => {
    positions.push(p[0], p[1], p[2]);
    vnormals.push(n[0], n[1], n[2]);
    uvs.push(u, v);
    return uvs.length / 2 - 1;
  };
  const at = (k, n) => add(core[k], scale(n, margin));

  // Flat faces: the core faces pushed out by the margin.
  const faces = rings.map((idx, cell) => {
    const n = normals[cell], poly = idx.map(k => at(k, n));
    const cornerValues = sides === 4 ? idx.map(k => values[rings.findIndex(r => !r.includes(k))]) : null;
    const face = { normal: n, value: values[cell], cell, idx, poly, ...layout(poly, n, sides, cornerValues, values[cell]) };
    const col = cell % ATLAS_COLS, row = Math.floor(cell / ATLAS_COLS), first = uvs.length / 2;
    face.flat.forEach(([x, y], k) => vertex(poly[k], n, (col + x) / ATLAS_COLS, 1 - (row + 1 - y) / rows));
    for (let k = 1; k < idx.length - 1; k++) indices.push(first, first + k, first + k + 1);
    return face;
  });

  // Rounded edges: cylinder strips around each core edge, normals sweeping from one face to the other.
  const edges = new Map(), around = core.map(() => []);
  rings.forEach((idx, f) => idx.forEach((a, k) => {
    const b = idx[(k + 1) % idx.length], key = Math.min(a, b) * 65536 + Math.max(a, b), e = edges.get(key);
    if (e) e.g = f;
    else { const edge = { a, b, f, g: -1 }; edges.set(key, edge); around[a].push(edge); around[b].push(edge); }
  }));
  for (const e of edges.values()) {
    const n0 = normals[e.f], n1 = normals[e.g], angle = Math.acos(Math.min(1, dot(n0, n1)));
    const steps = Math.max(1, Math.ceil(angle / (Math.PI / 10)));
    e.arc = Array.from({ length: steps + 1 }, (_, j) => j === 0 ? n0 : j === steps ? n1 : slerp(n0, n1, angle, j / steps));
    const A = e.arc.map(n => vertex(at(e.a, n), n, edgeU, edgeV)), B = e.arc.map(n => vertex(at(e.b, n), n, edgeU, edgeV));
    for (let j = 0; j < steps; j++) indices.push(A[j], A[j + 1], B[j + 1], A[j], B[j + 1], B[j]);
  }

  // Rounded corners: sphere patches fanned from the corner's mean normal, bounded by the edge arcs.
  around.forEach((list, a) => {
    const next = new Map();
    let c = [0, 0, 0];
    for (const e of list) {
      c = add(c, add(normals[e.f], normals[e.g]));
      if (e.a === a) next.set(e.g, { to: e.f, arc: [...e.arc].reverse() });
      else next.set(e.f, { to: e.g, arc: e.arc });
    }
    c = norm(c);
    const center = vertex(at(a, c), c, edgeU, edgeV);
    for (let k = 0, f = list[0].f; k < list.length; k++) {
      const { to, arc } = next.get(f), ids = arc.map(n => vertex(at(a, n), n, edgeU, edgeV));
      for (let j = 0; j < ids.length - 1; j++) indices.push(center, ids[j], ids[j + 1]);
      f = to;
      if (f === list[0].f) break;
    }
  });

  return {
    sides, bottomRead: sides === 4, faces, rows, margin, inradius,
    radius: Math.max(...core.map(v => Math.hypot(...v))) + margin,
    vertices: new Float32Array(core.flat()),
    positions: new Float32Array(positions), normals: new Float32Array(vnormals), uvs: new Float32Array(uvs),
    indices: new Uint16Array(indices),
  };
}

// Shapes are immutable and shared by every die of the same type.
const built = new Map();
export function buildDie(sides) {
  if (!built.has(sides)) built.set(sides, make(sides));
  return built.get(sides);
}

// Value a settled die shows from its column-major rotation: the face pointing up, or for the d4 the face it
// rests on (the number at the top apex).
export function readValue(shape, m) {
  let best = 0, bestDot = -Infinity;
  for (const face of shape.faces) {
    const n = face.normal, up = m[1] * n[0] + m[4] * n[1] + m[7] * n[2], d = shape.bottomRead ? -up : up;
    if (d > bestDot) { bestDot = d; best = face.value; }
  }
  return best;
}
