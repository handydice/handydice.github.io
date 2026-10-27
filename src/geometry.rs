//! Geometry of physical dice (port of dice3d.js). Every die is the solid {x : n·x ≤ 1} of its unit face
//! normals: all faces lie at the same distance from the center, so faces are planar by construction and the
//! platonic dice and the d10 are isohedral. The solid is shrunk to a core and rounded by a ball of radius
//! `margin` (core ⊕ ball), like the bevelled edges and corners of real plastic dice. The physics collides
//! exactly that shape, so rendered dice neither float nor sink.
use glam::{Quat, Vec3};

pub const ATLAS_COLS: u32 = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub value: u32,
    /// Position in the face's atlas cell, x right, y up, 0..1.
    pub x: f32,
    pub y: f32,
    /// Unit direction toward the label's top, in cell coordinates.
    pub up: [f32; 2],
    /// Radius of the free circle around the label, in cell units.
    pub size: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub normal: [f32; 3],
    /// Geometric value: face `value` shows `face_values[value - 1]` once a die is renumbered.
    pub value: u32,
    /// Atlas cell index.
    pub cell: u32,
    /// Face polygon (CCW from outside) at the rest height `inradius`.
    pub poly: Vec<[f32; 3]>,
    pub labels: Vec<Label>,
    /// Indices of the polygon corners into `Shape::vertices` (the core).
    pub idx: Vec<usize>,
    /// Polygon corners in the atlas cell (0..1), same order as `poly`.
    pub flat: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub sides: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub faces: Vec<Face>,
    /// Core vertices (the rounded die is core ⊕ ball of radius `margin`).
    pub vertices: Vec<[f32; 3]>,
    pub margin: f32,
    pub inradius: f32,
    pub radius: f32,
    pub rows: u32,
    /// A d4 is read from the face it rests on (the number at its apex).
    pub bottom_read: bool,
}

pub(crate) type V = [f64; 3];

const PHI: f64 = 1.618_033_988_749_895;
const PI: f64 = std::f64::consts::PI;

pub(crate) fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn scale(v: V, s: f64) -> V {
    [v[0] * s, v[1] * s, v[2] * s]
}
pub(crate) fn len(v: V) -> f64 {
    dot(v, v).sqrt()
}
fn norm(v: V) -> V {
    scale(v, 1.0 / len(v))
}
fn f32s(v: V) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

// ---- convex solid from face normals ----
fn clip(poly: &[V], m: V) -> Vec<V> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (dp, dq) = (dot(p, m) - 1.0, dot(q, m) - 1.0);
        if dp <= 0.0 {
            out.push(p);
        }
        if dp * dq < 0.0 {
            out.push(add(p, scale(sub(q, p), dp / (dp - dq))));
        }
    }
    out
}

/// Face polygons, CCW seen from outside: a big square in each face plane clipped by all other planes.
fn face_polys(normals: &[V]) -> Vec<Vec<V>> {
    normals
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let t = norm(cross(
                if n[1].abs() < 0.9 {
                    [0.0, 1.0, 0.0]
                } else {
                    [1.0, 0.0, 0.0]
                },
                n,
            ));
            let b = cross(n, t);
            let mut poly: Vec<V> = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
                .iter()
                .map(|&(u, v)| add(n, add(scale(t, 4.0 * u), scale(b, 4.0 * v))))
                .collect();
            for (j, &m) in normals.iter().enumerate() {
                if j != i {
                    poly = clip(&poly, m);
                }
            }
            poly
        })
        .collect()
}

/// Shared vertices: each corner is computed once per face that meets there.
fn weld(polys: &[Vec<V>]) -> (Vec<V>, Vec<Vec<usize>>) {
    let mut verts: Vec<V> = Vec::new();
    let rings = polys
        .iter()
        .map(|poly| {
            let mut idx: Vec<usize> = Vec::new();
            for &p in poly {
                let i = match verts.iter().position(|v| {
                    (v[0] - p[0]).abs() + (v[1] - p[1]).abs() + (v[2] - p[2]).abs() < 1e-7
                }) {
                    Some(i) => i,
                    None => {
                        verts.push(p);
                        verts.len() - 1
                    }
                };
                if idx.last() != Some(&i) {
                    idx.push(i);
                }
            }
            if idx.len() > 1 && idx[0] == idx[idx.len() - 1] {
                idx.pop();
            }
            idx
        })
        .collect();
    (verts, rings)
}

fn centroid(poly: &[V]) -> V {
    let (mut c, mut area) = ([0.0; 3], 0.0);
    for i in 1..poly.len() - 1 {
        let a = len(cross(sub(poly[i], poly[0]), sub(poly[i + 1], poly[0])));
        area += a;
        c = add(c, scale(add(add(poly[0], poly[i]), poly[i + 1]), a / 3.0));
    }
    scale(c, 1.0 / area)
}

// ---- the dice: face normals and numbering ----
/// Opposite faces sum to sides+1; preset values (non-zero) stay, their opposites follow.
fn number(normals: &[V], sides: u32, mut values: Vec<u32>) -> Vec<u32> {
    values.resize(normals.len(), 0);
    let mut next = 1;
    for (i, &n) in normals.iter().enumerate() {
        if values[i] != 0 {
            continue;
        }
        let j = normals
            .iter()
            .position(|&m| dot(m, n) < -0.999999)
            .expect("opposite face");
        if values[j] != 0 {
            values[i] = sides + 1 - values[j];
        } else {
            while values.contains(&next) {
                next += 1;
            }
            values[i] = next;
            values[j] = sides + 1 - next;
        }
    }
    values
}

fn corners() -> Vec<V> {
    let mut out = Vec::with_capacity(8);
    for x in [1.0, -1.0] {
        for y in [1.0, -1.0] {
            for z in [1.0, -1.0] {
                out.push([x, y, z]);
            }
        }
    }
    out
}
fn signed_cyclic(a: f64, b: f64) -> Vec<V> {
    [(a, b), (a, -b), (-a, b), (-a, -b)]
        .iter()
        .flat_map(|&(a, b)| [[0.0, a, b], [a, b, 0.0], [b, 0.0, a]])
        .collect()
}

/// Pentagonal trapezohedron: kite normals tilted θ from the poles, the lower ring turned by 36°. All ten
/// faces are congruent planar kites; pole-to-pole height / belt width ≈ 1.1, like real d10s.
fn d10_normals() -> Vec<V> {
    let theta = (1.1 * 2.0 / (1.0 + (PI / 5.0).cos())).atan();
    let (s, c) = (theta.sin(), theta.cos());
    let mut out = Vec::with_capacity(10);
    for up in [1.0, -1.0] {
        for k in 0..5 {
            let a = (2 * k + usize::from(up < 0.0)) as f64 * PI / 5.0;
            out.push([s * a.cos(), up * c, s * a.sin()]);
        }
    }
    out
}

/// d100: a convex die with 100 faces in 50 opposite pairs (like a Zocchihedron, but with flat faces so it
/// rests on one face). Lloyd relaxation of a Fibonacci spiral evens out the face sizes; it is not
/// isohedral, so faces are only nearly alike.
fn d100_normals() -> Vec<V> {
    let ga = PI * (3.0 - 5f64.sqrt());
    let mut pts: Vec<V> = (0..50)
        .map(|i| {
            let y = 1.0 - (i as f64 + 0.5) / 50.0;
            let r = (1.0 - y * y).sqrt();
            let a = i as f64 * ga;
            [r * a.cos(), y, r * a.sin()]
        })
        .collect();
    let both = |pts: &[V]| -> Vec<V> { pts.iter().flat_map(|&p| [p, scale(p, -1.0)]).collect() };
    for _ in 0..30 {
        pts = face_polys(&both(&pts))
            .iter()
            .step_by(2)
            .map(|poly| norm(centroid(poly)))
            .collect();
    }
    both(&pts)
}

fn solid(sides: u32) -> (Vec<V>, Vec<u32>) {
    match sides {
        4 => (
            [
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, -1.0],
            ]
            .map(norm)
            .to_vec(),
            vec![1, 2, 3, 4],
        ),
        // 1, 2, 3 counterclockwise around their corner, as on Western dice.
        6 => (
            vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0],
            ],
            vec![1, 2, 3, 4, 5, 6],
        ),
        8 => (corners().into_iter().map(norm).collect(), vec![]),
        // Odd numbers around the upper pole, even around the lower one.
        10 => (d10_normals(), vec![1, 7, 3, 9, 5]),
        12 => (
            signed_cyclic(1.0, PHI).into_iter().map(norm).collect(),
            vec![],
        ),
        20 => {
            let mut n = corners();
            n.extend(signed_cyclic(1.0 / PHI, PHI));
            (n.into_iter().map(norm).collect(), vec![])
        }
        100 => (d100_normals(), vec![]),
        _ => panic!("no d{sides}"),
    }
}

/// Sharp circumradius and bevel radius in world units (a d6 is 0.9 wide).
fn size(sides: u32) -> (f64, f64) {
    match sides {
        4 => (0.68, 0.06),
        6 => (0.78, 0.07),
        8 => (0.66, 0.055),
        10 => (0.66, 0.05),
        12 => (0.68, 0.05),
        20 => (0.7, 0.045),
        _ => (0.8, 0.03),
    }
}

/// Face texture layout: an orthonormal basis in the face plane with "up" toward the label's top, the face
/// scaled uniformly into its atlas cell (x right, y up, 0..1), and the labels to print.
fn layout(
    poly: &[V],
    n: V,
    sides: u32,
    corner_values: Option<&[u32]>,
    value: u32,
) -> (Vec<[f32; 2]>, Vec<Label>) {
    let c = centroid(poly);
    let up = if sides == 6 {
        scale(add(poly[0], poly[1]), 0.5)
    } else {
        poly.iter().copied().fold(poly[0], |best, p| {
            if len(sub(p, c)) > len(sub(best, c)) + 1e-9 {
                p
            } else {
                best
            }
        })
    };
    let uc = sub(up, c);
    let bv = norm(sub(uc, scale(n, dot(uc, n))));
    let tv = cross(bv, n);
    let local = |p: V| [dot(sub(p, c), tv), dot(sub(p, c), bv)];
    let pts: Vec<[f64; 2]> = poly.iter().map(|&p| local(p)).collect();
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]) / 0.96;
    let to_cell = |p: [f64; 2]| {
        [
            0.5 + (p[0] - (lo[0] + hi[0]) / 2.0) / span,
            0.5 + (p[1] - (lo[1] + hi[1]) / 2.0) / span,
        ]
    };
    // Radius of the free circle around a point: its distance to the nearest edge.
    let free = |q: [f64; 2]| {
        (0..pts.len())
            .map(|k| {
                let (p, r) = (pts[k], pts[(k + 1) % pts.len()]);
                let e = [r[0] - p[0], r[1] - p[1]];
                (e[0] * (q[1] - p[1]) - e[1] * (q[0] - p[0])).abs() / e[0].hypot(e[1])
            })
            .fold(f64::INFINITY, f64::min)
            / span
    };
    let label = |v: u32, at: V, toward: V| {
        let q = local(at);
        let t = local(toward);
        let d = [t[0] - q[0], t[1] - q[1]];
        let l = d[0].hypot(d[1]);
        let [x, y] = to_cell(q);
        Label {
            value: v,
            x: x as f32,
            y: y as f32,
            up: [(d[0] / l) as f32, (d[1] / l) as f32],
            size: free(q) as f32,
        }
    };
    // A d4 is read at its apex: each corner shows the value of the face opposite that corner.
    let labels = match corner_values {
        Some(cv) => poly
            .iter()
            .enumerate()
            .map(|(k, &p)| label(cv[k], add(c, scale(sub(p, c), 0.55)), p))
            .collect(),
        None => vec![label(value, c, add(c, bv))],
    };
    let flat = pts.iter().map(|&p| {
        let [x, y] = to_cell(p);
        [x as f32, y as f32]
    });
    (flat.collect(), labels)
}

fn slerp(a: V, b: V, angle: f64, t: f64) -> V {
    let s = angle.sin();
    add(
        scale(a, ((1.0 - t) * angle).sin() / s),
        scale(b, (t * angle).sin() / s),
    )
}

struct Edge {
    a: usize,
    b: usize,
    f: usize,
    g: usize,
    arc: Vec<V>,
}

struct Mesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

impl Mesh {
    fn vertex(&mut self, p: V, n: V, u: f64, v: f64) -> u32 {
        self.positions.push(f32s(p));
        self.normals.push(f32s(n));
        self.uvs.push([u as f32, v as f32]);
        (self.uvs.len() - 1) as u32
    }
}

/// Builds the immutable shape of a die type. Callers cache it per side count (the physics does).
pub fn build_die(sides: u32) -> Shape {
    let (normals, preset) = solid(sides);
    let values = if sides == 4 {
        preset
    } else {
        number(&normals, sides, preset)
    };
    let (r, margin) = size(sides);
    let (verts, rings) = weld(&face_polys(&normals));
    let inradius = r / verts.iter().map(|&v| len(v)).fold(0.0, f64::max);
    let core: Vec<V> = verts.iter().map(|&v| scale(v, inradius - margin)).collect();
    let f = normals.len() as u32;
    let cols = ATLAS_COLS as f64;
    let rows = (f + 1).div_ceil(ATLAS_COLS);
    let edge_u = ((f % ATLAS_COLS) as f64 + 0.5) / cols;
    let edge_v = 1.0 - ((f / ATLAS_COLS) as f64 + 0.5) / rows as f64;
    let mut mesh = Mesh {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
    };
    let mut indices: Vec<u32> = Vec::new();
    let at = |k: usize, n: V| add(core[k], scale(n, margin));

    // Flat faces: the core faces pushed out by the margin.
    let faces: Vec<Face> = rings
        .iter()
        .enumerate()
        .map(|(cell, idx)| {
            let n = normals[cell];
            let poly: Vec<V> = idx.iter().map(|&k| at(k, n)).collect();
            let corner_values: Option<Vec<u32>> = (sides == 4).then(|| {
                idx.iter()
                    .map(|&k| values[rings.iter().position(|r| !r.contains(&k)).unwrap()])
                    .collect()
            });
            let (flat, labels) = layout(&poly, n, sides, corner_values.as_deref(), values[cell]);
            let (col, row) = (cell as f64 % cols, (cell as u32 / ATLAS_COLS) as f64);
            let first = mesh.uvs.len() as u32;
            for (k, &[x, y]) in flat.iter().enumerate() {
                mesh.vertex(
                    poly[k],
                    n,
                    (col + x as f64) / cols,
                    1.0 - (row + 1.0 - y as f64) / rows as f64,
                );
            }
            for k in 1..idx.len() as u32 - 1 {
                indices.extend([first, first + k, first + k + 1]);
            }
            Face {
                normal: f32s(n),
                value: values[cell],
                cell: cell as u32,
                poly: poly.iter().map(|&p| f32s(p)).collect(),
                labels,
                idx: idx.clone(),
                flat,
            }
        })
        .collect();

    // Rounded edges: cylinder strips around each core edge, normals sweeping from one face to the other.
    let mut edges: Vec<Edge> = Vec::new();
    let mut keys: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); core.len()];
    for (fi, idx) in rings.iter().enumerate() {
        for (k, &a) in idx.iter().enumerate() {
            let b = idx[(k + 1) % idx.len()];
            match keys.entry((a.min(b), a.max(b))) {
                std::collections::hash_map::Entry::Occupied(e) => edges[*e.get()].g = fi,
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(edges.len());
                    around[a].push(edges.len());
                    around[b].push(edges.len());
                    edges.push(Edge {
                        a,
                        b,
                        f: fi,
                        g: usize::MAX,
                        arc: Vec::new(),
                    });
                }
            }
        }
    }
    for e in &mut edges {
        let (n0, n1) = (normals[e.f], normals[e.g]);
        let angle = dot(n0, n1).min(1.0).acos();
        let steps = ((angle / (PI / 10.0)).ceil() as usize).max(1);
        e.arc = (0..=steps)
            .map(|j| {
                if j == 0 {
                    n0
                } else if j == steps {
                    n1
                } else {
                    slerp(n0, n1, angle, j as f64 / steps as f64)
                }
            })
            .collect();
        let a: Vec<u32> = e
            .arc
            .iter()
            .map(|&n| mesh.vertex(at(e.a, n), n, edge_u, edge_v))
            .collect();
        let b: Vec<u32> = e
            .arc
            .iter()
            .map(|&n| mesh.vertex(at(e.b, n), n, edge_u, edge_v))
            .collect();
        for j in 0..steps {
            indices.extend([a[j], a[j + 1], b[j + 1], a[j], b[j + 1], b[j]]);
        }
    }

    // Rounded corners: sphere patches fanned from the corner's mean normal, bounded by the edge arcs.
    for (a, list) in around.iter().enumerate() {
        // (from face, to face, edge, reversed arc)
        let mut next: Vec<(usize, usize, usize, bool)> = Vec::with_capacity(list.len());
        let mut c = [0.0; 3];
        for &ei in list {
            let e = &edges[ei];
            c = add(c, add(normals[e.f], normals[e.g]));
            next.push(if e.a == a {
                (e.g, e.f, ei, true)
            } else {
                (e.f, e.g, ei, false)
            });
        }
        let c = norm(c);
        let center = mesh.vertex(at(a, c), c, edge_u, edge_v);
        let start = edges[list[0]].f;
        let mut f = start;
        for _ in 0..list.len() {
            let &(_, to, ei, reversed) = next.iter().find(|x| x.0 == f).expect("closed corner");
            let arc = &edges[ei].arc;
            let ids: Vec<u32> = if reversed {
                arc.iter()
                    .rev()
                    .map(|&n| mesh.vertex(at(a, n), n, edge_u, edge_v))
                    .collect()
            } else {
                arc.iter()
                    .map(|&n| mesh.vertex(at(a, n), n, edge_u, edge_v))
                    .collect()
            };
            for j in 0..ids.len() - 1 {
                indices.extend([center, ids[j], ids[j + 1]]);
            }
            f = to;
            if f == start {
                break;
            }
        }
    }

    Shape {
        sides,
        bottom_read: sides == 4,
        faces,
        rows,
        margin: margin as f32,
        inradius: inradius as f32,
        radius: (core.iter().map(|&v| len(v)).fold(0.0, f64::max) + margin) as f32,
        vertices: core.iter().map(|&v| f32s(v)).collect(),
        positions: mesh.positions,
        normals: mesh.normals,
        uvs: mesh.uvs,
        indices,
    }
}

/// Geometric value a settled die shows: the face pointing up, or for the d4 the face it rests on (the
/// number at the top apex). Map it through the die's face values for the printed number.
pub fn read_value(shape: &Shape, rotation: Quat) -> u32 {
    let mut best = 0;
    let mut best_dot = f32::NEG_INFINITY;
    for face in &shape.faces {
        let up = (rotation * Vec3::from(face.normal)).y;
        let d = if shape.bottom_read { -up } else { up };
        if d > best_dot {
            best_dot = d;
            best = face.value;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn v(p: [f32; 3]) -> V {
        [p[0] as f64, p[1] as f64, p[2] as f64]
    }

    /// Solid angle of a planar polygon seen from the center (Van Oosterom–Strackee, fanned).
    fn solid_angle(poly: &[V]) -> f64 {
        (1..poly.len() - 1)
            .map(|i| {
                let (a, b, c) = (poly[0], poly[i], poly[i + 1]);
                let (la, lb, lc) = (len(a), len(b), len(c));
                2.0 * dot(a, cross(b, c))
                    .abs()
                    .atan2(la * lb * lc + dot(a, b) * lc + dot(a, c) * lb + dot(b, c) * la)
            })
            .sum()
    }

    /// All seven dice: numbering, planar faces on a convex core, atlas layout, closed outward mesh, and
    /// reading every face; plus the d4 apex labels and the relaxed d100.
    #[test]
    fn dice_meshes_are_closed_numbered_and_readable() {
        for sides in [4, 6, 8, 10, 12, 20, 100] {
            let die = build_die(sides);
            let faces = &die.faces;
            assert_eq!(faces.len(), sides as usize);
            let mut values: Vec<u32> = faces.iter().map(|f| f.value).collect();
            values.sort();
            assert_eq!(
                values,
                (1..=sides).collect::<Vec<_>>(),
                "d{sides} numbers 1..{sides} once"
            );
            assert_eq!(die.rows, (sides + 1).div_ceil(ATLAS_COLS));
            assert!(
                die.margin > 0.0
                    && die.inradius > die.margin
                    && die.radius > die.inradius
                    && die.radius < 0.9
            );

            let (inradius, margin) = (die.inradius as f64, die.margin as f64);
            for f in faces {
                let n = v(f.normal);
                assert!(
                    f.poly.len() >= 3
                        && f.poly.len() == f.idx.len()
                        && f.poly.len() == f.flat.len()
                );
                for &p in &f.poly {
                    assert!(
                        (dot(v(p), n) - inradius).abs() < 1e-5,
                        "d{sides} face {} is flat",
                        f.value
                    );
                }
                for &p in &die.vertices {
                    assert!(
                        dot(v(p), n) <= inradius - margin + 1e-5,
                        "d{sides} core is convex"
                    );
                }
                assert!(f
                    .flat
                    .iter()
                    .all(|&[x, y]| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
                assert!(f.labels.iter().all(|l| l.x > 0.0
                    && l.x < 1.0
                    && l.y > 0.0
                    && l.y < 1.0
                    && l.size > 0.05
                    && (l.up[0].hypot(l.up[1]) - 1.0).abs() < 1e-5));
                if sides != 4 {
                    let o = faces
                        .iter()
                        .find(|g| dot(n, v(g.normal)) < -0.9999)
                        .expect("opposite face");
                    assert_eq!(f.value + o.value, sides + 1, "d{sides} opposite faces");
                    assert_eq!(f.labels.len(), 1);
                }
                // Turning any face up reads its value; a d4 is read from the face it rests on.
                let to = if die.bottom_read {
                    Vec3::NEG_Y
                } else {
                    Vec3::Y
                };
                assert_eq!(
                    read_value(&die, Quat::from_rotation_arc(Vec3::from(f.normal), to)),
                    f.value
                );
            }

            // Closed (every edge used once per direction), outward, finite, unit normals, uvs in the atlas.
            let (p, n, idx) = (&die.positions, &die.normals, &die.indices);
            assert!(idx.len() % 3 == 0 && idx.iter().all(|&i| (i as usize) < p.len()));
            let key = |i: u32| p[i as usize].map(f32::to_bits);
            let mut directed: HashMap<([u32; 3], [u32; 3]), u32> = HashMap::new();
            let mut volume = 0.0;
            for t in idx.chunks(3) {
                let [a, b, c] = [
                    v(p[t[0] as usize]),
                    v(p[t[1] as usize]),
                    v(p[t[2] as usize]),
                ];
                for k in 0..3 {
                    let (u, w) = (key(t[k]), key(t[(k + 1) % 3]));
                    if u != w {
                        *directed.entry((u, w)).or_default() += 1;
                    }
                }
                volume += dot(a, cross(b, c)) / 6.0;
                let face = cross(sub(b, a), sub(c, a));
                if len(face) > 1e-7 {
                    let vn = t.iter().fold([0.0; 3], |s, &i| add(s, v(n[i as usize])));
                    assert!(dot(face, vn) > 0.0, "d{sides} triangle winds outward");
                }
            }
            for (&(u, w), &count) in &directed {
                assert_eq!(count, 1, "d{sides} edge used once per direction");
                assert_eq!(directed.get(&(w, u)), Some(&1), "d{sides} mesh is closed");
            }
            assert!(volume > 0.0);
            assert!(p.iter().all(|q| q.iter().all(|c| c.is_finite())));
            assert!(n.iter().all(|&q| (len(v(q)) - 1.0).abs() < 1e-5));
            assert!(die
                .uvs
                .iter()
                .all(|uv| uv.iter().all(|c| (0.0..=1.0).contains(c))));
        }

        // d4: each corner shows the value of the face opposite it, so the apex shows the rolled value.
        let d4 = build_die(4);
        for f in &d4.faces {
            assert_eq!(f.labels.len(), 3);
            for (k, label) in f.labels.iter().enumerate() {
                let opposite = d4
                    .faces
                    .iter()
                    .find(|g| !g.idx.contains(&f.idx[k]))
                    .unwrap();
                assert_eq!(label.value, opposite.value, "apex numbering");
            }
        }

        // d6: 1, 2, 3 counterclockwise around their corner. d10: odd around the upper pole.
        let d6 = build_die(6);
        let n6 = |value| v(d6.faces.iter().find(|f| f.value == value).unwrap().normal);
        assert!(dot(n6(1), cross(n6(2), n6(3))) > 0.99);
        for f in &build_die(10).faces {
            assert_eq!(f.poly.len(), 4);
            assert_eq!(f.normal[1] > 0.0, f.value % 2 == 1);
        }

        // d100: one hundred real faces that tile the sphere of directions, nearly equal, with room for digits.
        let d100 = build_die(100);
        let angles: Vec<f64> = d100
            .faces
            .iter()
            .map(|f| {
                solid_angle(
                    &f.idx
                        .iter()
                        .map(|&k| v(d100.vertices[k]))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert!((angles.iter().sum::<f64>() - 4.0 * PI).abs() < 1e-3);
        let spread = angles.iter().cloned().fold(0.0, f64::max)
            / angles.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(spread < 1.3, "d100 faces nearly alike ({spread})");
        assert!(d100
            .faces
            .iter()
            .all(|f| f.poly.len() >= 4 && f.labels[0].size > 0.2));
    }
}
