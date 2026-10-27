//! Rigid-body dice (port of physics3d.js + the scene logic of scene3d.js). A die collides as its rounded
//! core (convex core ⊕ ball of radius `shape.margin`), exactly the rendered shape: GJK distance between the
//! cores, reference-face clipping for resting manifolds, vertex contacts with the table and the walls,
//! sequential impulses with Coulomb friction and restitution, and sleeping. Deterministic and
//! allocation-free per step once warmed up: randomness lives only in the initial conditions.
//!
//! World: X right, Y up, Z screen down; the table top is y = 0, walls are infinitely high.
use std::sync::Arc;

use glam::{Quat, Vec3};

use crate::geometry::{add, build_die, cross, dot, len, read_value, scale, sub, Shape, V};
use crate::model::{random, random_face_values, Die};

const GRAVITY: f64 = 32.0; // world units/s²; a d6 is 0.9 wide
const SUBSTEP: f64 = 1.0 / 240.0;
const ITERATIONS: usize = 10;
const SPECULATIVE: f64 = 0.03; // contacts start this far before touching, so fast dice cannot pass through
const SLOP: f64 = 0.002; // tolerated penetration, keeps resting contacts calm
const BOUNCE: f64 = 0.8; // slower impacts do not bounce (no resting jitter)
struct Material {
    e: f64,
    mu: f64,
}
const TABLE: Material = Material { e: 0.3, mu: 0.55 };
const WALL: Material = Material { e: 0.5, mu: 0.25 };
const DIE: Material = Material { e: 0.35, mu: 0.3 };
const SLEEP_SPEED: f64 = 0.1;
const SLEEP_SPIN: f64 = 0.3;
const SLEEP_TIME: f64 = 0.3;
const WAKE_SPEED: f64 = 0.25; // a sleeping die hit by a slower die stays asleep and holds it like a wall
const HOLD_GAIN: f64 = 18.0;
const HOLD_MAX: f64 = 25.0;

// Scene tuning (scene3d.js). Gentle translation in world units/s; fast tumbling in radians/s.
// SPIN_DECAY: per-second exponential slowdown of a finger spin on a held die.
const FRAME: f64 = 1.0 / 120.0;
const MOVE_SPEED: f64 = 2.0;
const FLICK_SPEED: f64 = 20.0;
const MAX_FLICK_SPIN: f64 = 18.0;
const RANDOMIZE_SPIN: f64 = 16.0;
const SPIN_DECAY: f64 = 0.7;
const TILT_DEADBAND: f64 = 0.01;
const HELD_HEIGHT: f64 = 1.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub id: u32,
    pub position: Vec3,
    /// Body → world rotation.
    pub rotation: Quat,
}

type Q = [f64; 4]; // x, y, z, w

fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(x))
}

/// Column-major rotation matrix of a unit quaternion.
fn mat3(q: Q) -> [f64; 9] {
    let [x, y, z, w] = q;
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ]
}
fn rotate(m: &[f64; 9], v: V) -> V {
    [
        m[0] * v[0] + m[3] * v[1] + m[6] * v[2],
        m[1] * v[0] + m[4] * v[1] + m[7] * v[2],
        m[2] * v[0] + m[5] * v[1] + m[8] * v[2],
    ]
}
fn unrotate(m: &[f64; 9], v: V) -> V {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}
fn normalize_q(q: &mut Q) {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    for c in q.iter_mut() {
        *c /= l;
    }
}

/// Uniformly random orientation (Shoemake), from any random() in [0,1).
fn random_quat(random: &mut dyn FnMut() -> f64) -> Q {
    let u1 = random();
    let u2 = random() * std::f64::consts::TAU;
    let u3 = random() * std::f64::consts::TAU;
    let (a, b) = ((1.0 - u1).sqrt(), u1.sqrt());
    [a * u2.sin(), a * u2.cos(), b * u3.sin(), b * u3.cos()]
}

/// Cryptographic uniform float in [0, 1).
fn crypto_unit() -> f64 {
    getrandom::u32().expect("crypto random") as f64 / 4_294_967_296.0
}

#[cfg(target_arch = "wasm32")]
fn now() -> f64 {
    js_sys::Date::now() / 1000.0
}
#[cfg(not(target_arch = "wasm32"))]
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// A shape with f64 copies of what the solver reads.
struct Solid {
    shape: Shape,
    verts: Vec<V>,
    normals: Vec<V>,
    /// Hull plane distance per face (core plane grown by the margin), for picking.
    planes: Vec<f64>,
}

impl Solid {
    fn new(sides: u32) -> Self {
        let shape = build_die(sides);
        let v3 = |v: [f32; 3]| [v[0] as f64, v[1] as f64, v[2] as f64];
        let verts: Vec<V> = shape.vertices.iter().map(|&v| v3(v)).collect();
        let normals: Vec<V> = shape.faces.iter().map(|f| v3(f.normal)).collect();
        let planes = normals
            .iter()
            .map(|&n| {
                verts
                    .iter()
                    .map(|&v| dot(n, v))
                    .fold(f64::NEG_INFINITY, f64::max)
                    + shape.margin as f64
            })
            .collect();
        Solid {
            shape,
            verts,
            normals,
            planes,
        }
    }
    fn margin(&self) -> f64 {
        self.shape.margin as f64
    }
    fn radius(&self) -> f64 {
        self.shape.radius as f64
    }
}

struct Held {
    target: V,
    ang: V,
    quat: Q,
}

struct Body {
    id: u32,
    solid: Arc<Solid>,
    /// Printed number of geometric face k is face_values[k - 1].
    face_values: Vec<u32>,
    /// Tray the body was last placed for (physics side); `in_tray` mirrors the model.
    tray: Option<String>,
    in_tray: bool,
    active: bool,
    pos: V,
    quat: Q,
    vel: V,
    ang: V,
    sleeping: bool,
    sleep_timer: f64,
    held: Option<Held>,
    guided: bool,
    randomize_on_spin: bool,
    inv_inertia: f64,
    im: f64,
    ii: f64,
    touch: bool,
    /// Dice it slept against.
    rest: Vec<u32>,
    last_impact: f64,
    last_clack: f64,
    m: [f64; 9],
    world: Vec<V>,
}

impl Body {
    fn new(id: u32, solid: Arc<Solid>, pos: V) -> Self {
        // Equal masses; inertia of a ball between in- and circumradius (exact enough for these round solids).
        let k = (solid.shape.inradius + solid.shape.radius) as f64 / 2.0;
        let mut b = Body {
            id,
            face_values: (1..=solid.shape.sides).collect(),
            world: vec![[0.0; 3]; solid.verts.len()],
            solid,
            tray: None,
            in_tray: false,
            active: false,
            pos,
            quat: [0.0, 0.0, 0.0, 1.0],
            vel: [0.0; 3],
            ang: [0.0; 3],
            sleeping: false,
            sleep_timer: 0.0,
            held: None,
            guided: false,
            randomize_on_spin: false,
            inv_inertia: 1.0 / (0.4 * k * k),
            im: 1.0,
            ii: 0.0,
            touch: false,
            rest: Vec::new(),
            last_impact: -1.0,
            last_clack: -1.0,
            m: [0.0; 9],
        };
        b.place();
        b
    }

    fn place(&mut self) {
        self.m = mat3(self.quat);
        for (w, &v) in self.world.iter_mut().zip(self.solid.verts.iter()) {
            *w = add(rotate(&self.m, v), self.pos);
        }
    }

    fn rotation(&self) -> Quat {
        Quat::from_xyzw(
            self.quat[0] as f32,
            self.quat[1] as f32,
            self.quat[2] as f32,
            self.quat[3] as f32,
        )
    }

    fn pose(&self) -> Pose {
        let p = self.pos;
        Pose {
            id: self.id,
            position: Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32),
            rotation: self.rotation(),
        }
    }

    /// Printed number on the face read the way the renderer shows it.
    fn displayed(&self) -> u32 {
        self.face_values[read_value(&self.solid.shape, self.rotation()) as usize - 1]
    }

    fn renumber(&mut self) {
        self.face_values = random_face_values(self.solid.shape.sides);
    }

    fn stop(&mut self) {
        self.vel = [0.0; 3];
        self.ang = [0.0; 3];
        self.sleeping = true;
    }

    /// Turn the die so geometric face `value` is up (a d4: the face it rests on, its apex reading), keeping
    /// its heading and x/z, and set it down at rest on the table.
    fn orient_value(&mut self, value: u32) {
        let shape = &self.solid.shape;
        let Some(face) = shape.faces.iter().position(|f| f.value == value) else {
            return;
        };
        let t = if shape.bottom_read { -1.0 } else { 1.0 };
        let w = rotate(&mat3(self.quat), self.solid.normals[face]);
        // Shortest rotation of the face normal onto (0, t, 0): axis w × (0, t, 0).
        let (ax, az) = (-w[2] * t, w[0] * t);
        let s = ax.hypot(az);
        let c = w[1] * t;
        let angle = s.atan2(c);
        let h = (angle / 2.0).sin();
        let r = if s > 1e-9 {
            [ax / s * h, 0.0, az / s * h, (angle / 2.0).cos()]
        } else if c > 0.0 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0, 0.0]
        };
        let [x, y, z, w] = self.quat;
        self.quat = [
            r[3] * x + r[0] * w + r[1] * z - r[2] * y,
            r[3] * y - r[0] * z + r[1] * w + r[2] * x,
            r[3] * z + r[0] * y - r[1] * x + r[2] * w,
            r[3] * w - r[0] * x - r[1] * y - r[2] * z,
        ];
        normalize_q(&mut self.quat);
        self.pos[1] = self.solid.shape.inradius as f64;
        self.stop();
        self.sleep_timer = 0.0;
        self.rest.clear();
        self.guided = false;
        self.randomize_on_spin = false;
        self.place();
    }

    /// Show printed `value` under the current numbering, resting on the table.
    fn restore_value(&mut self, value: u32) {
        if let Some(k) = self.face_values.iter().position(|&v| v == value) {
            self.orient_value(k as u32 + 1);
        }
    }

    fn rouse(&mut self) {
        self.sleeping = false;
        self.sleep_timer = 0.0;
        self.rest.clear();
        self.im = 1.0;
        self.ii = self.inv_inertia;
    }

    fn moving(&self) -> bool {
        let r = self.solid.radius();
        self.held.is_some()
            || dot(self.vel, self.vel) + dot(self.ang, self.ang) * r * r > WAKE_SPEED * WAKE_SPEED
    }

    /// Face whose world normal is closest to direction d, with its cosine.
    fn best_face(&self, d: V) -> (usize, f64) {
        let l = unrotate(&self.m, d);
        let mut best = (0, f64::NEG_INFINITY);
        for (i, &n) in self.solid.normals.iter().enumerate() {
            let c = dot(n, l);
            if c > best.1 {
                best = (i, c);
            }
        }
        best
    }
}

/// q += ½ (ω, 0) ⊗ q · dt, renormalized; ω in world space.
fn spin(q: &mut Q, w: V, dt: f64) {
    let [x, y, z, s] = *q;
    let k = 0.5 * dt;
    q[0] = x + k * (w[0] * s + w[1] * z - w[2] * y);
    q[1] = y + k * (w[1] * s + w[2] * x - w[0] * z);
    q[2] = z + k * (w[2] * s + w[0] * y - w[1] * x);
    q[3] = s - k * (w[0] * x + w[1] * y + w[2] * z);
    normalize_q(q);
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: f64,
    z: f64,
    min_z: f64,
    max_z: f64,
}

// ---- contacts: pooled, normal from a to b (a None = static table or wall) ----
#[derive(Clone, Copy, Default)]
struct Contact {
    a: Option<usize>,
    b: usize,
    n: V,
    mu: f64,
    ra: V,
    rb: V,
    t1: V,
    t2: V,
    kn: f64,
    kt1: f64,
    kt2: f64,
    target: f64,
    ln: f64,
    lt1: f64,
    lt2: f64,
}

#[derive(Clone, Copy, Default)]
struct Group {
    a: Option<usize>,
    b: usize,
    n: V,
    depth: f64,
}

fn rel_vel(c: &Contact, bodies: &[Body]) -> V {
    let b = &bodies[c.b];
    let mut rv = add(b.vel, cross(b.ang, c.rb));
    if let Some(a) = c.a {
        let a = &bodies[a];
        rv = sub(rv, add(a.vel, cross(a.ang, c.ra)));
    }
    rv
}

/// Inverse effective mass along a direction (scalar inertia: im + ii·|r×d|² per body).
fn mass(c: &Contact, bodies: &[Body], d: V) -> f64 {
    let b = &bodies[c.b];
    let x = cross(c.rb, d);
    let mut k = b.im + b.ii * dot(x, x);
    if let Some(a) = c.a {
        let a = &bodies[a];
        let x = cross(c.ra, d);
        k += a.im + a.ii * dot(x, x);
    }
    k
}

fn impulse(c: &Contact, bodies: &mut [Body], p: V) {
    let b = &mut bodies[c.b];
    if b.im != 0.0 {
        b.vel = add(b.vel, scale(p, b.im));
        b.ang = add(b.ang, scale(cross(c.rb, p), b.ii));
    }
    if let Some(a) = c.a {
        let a = &mut bodies[a];
        if a.im != 0.0 {
            a.vel = sub(a.vel, scale(p, a.im));
            a.ang = sub(a.ang, scale(cross(c.ra, p), a.ii));
        }
    }
}

fn solve(c: &mut Contact, bodies: &mut [Body]) {
    let rv = rel_vel(c, bodies);
    let l = (c.ln + (c.target - dot(rv, c.n)) / c.kn).max(0.0);
    let d = l - c.ln;
    c.ln = l;
    if d != 0.0 {
        impulse(c, bodies, scale(c.n, d));
    }
    let max = c.mu * c.ln;
    let rv = rel_vel(c, bodies);
    let l = clamp(c.lt1 - dot(rv, c.t1) / c.kt1, -max, max);
    let d = l - c.lt1;
    c.lt1 = l;
    if d != 0.0 {
        impulse(c, bodies, scale(c.t1, d));
    }
    let rv = rel_vel(c, bodies);
    let l = clamp(c.lt2 - dot(rv, c.t2) / c.kt2, -max, max);
    let d = l - c.lt2;
    c.lt2 = l;
    if d != 0.0 {
        impulse(c, bodies, scale(c.t2, d));
    }
}

fn clip_poly(src: &[V], dst: &mut Vec<V>, n: V, off: f64) {
    dst.clear();
    for k in 0..src.len() {
        let (p, q) = (src[k], src[(k + 1) % src.len()]);
        let (dp, dq) = (dot(p, n) - off, dot(q, n) - off);
        if dp <= 0.0 {
            dst.push(p);
        }
        if dp * dq < 0.0 {
            dst.push(add(p, scale(sub(q, p), dp / (dp - dq))));
        }
    }
}

/// Separating-axis scan over P's face normals: the axis where Q penetrates P's face plane least.
fn scan(p: &Body, q: &Body) -> (f64, usize) {
    let mut least = (f64::INFINITY, 0);
    for (f, face) in p.solid.shape.faces.iter().enumerate() {
        let n = rotate(&p.m, p.solid.normals[f]);
        let min = q
            .world
            .iter()
            .map(|&v| dot(v, n))
            .fold(f64::INFINITY, f64::min);
        let depth = dot(p.world[face.idx[0]], n) - min;
        if depth < least.0 {
            least = (depth, f);
        }
    }
    least
}

fn support(w: &[V], d: V) -> usize {
    let mut top = (f64::NEG_INFINITY, 0);
    for (i, &v) in w.iter().enumerate() {
        let s = dot(v, d);
        if s > top.0 {
            top = (s, i);
        }
    }
    top.1
}

// ---- GJK distance between two cores (Ericson, Real-Time Collision Detection 9.5) ----
#[derive(Default)]
struct Gjk {
    sw: [V; 4],
    sa: [usize; 4],
    sb: [usize; 4],
    sl: [f64; 4],
    sn: usize,
    ri: [usize; 3],
    rl: [f64; 3],
    rn: usize,
    v: V,
}

const TET: [[usize; 4]; 4] = [[0, 1, 2, 3], [0, 2, 3, 1], [0, 3, 1, 2], [1, 3, 2, 0]];

impl Gjk {
    /// Distance between the cores with closest points on A and B; None when the cores overlap.
    fn distance(&mut self, wa: &[V], wb: &[V], pa: V, pb: V) -> Option<(f64, V, V)> {
        self.v = sub(pa, pb);
        if self.v == [0.0; 3] {
            self.v[0] = 1.0;
        }
        self.sn = 0;
        for _ in 0..64 {
            let v = self.v;
            let (ia, ib) = (support(wa, scale(v, -1.0)), support(wb, v));
            let w = sub(wa[ia], wb[ib]);
            let vv = dot(v, v);
            if self.sn > 0 && vv - dot(v, w) <= 1e-9 * vv {
                break; // no closer point exists
            }
            if (0..self.sn).any(|k| self.sa[k] == ia && self.sb[k] == ib) {
                break;
            }
            self.sw[self.sn] = w;
            self.sa[self.sn] = ia;
            self.sb[self.sn] = ib;
            self.sn += 1;
            if !self.reduce() || dot(self.v, self.v) < 1e-14 {
                return None;
            }
        }
        let (mut a, mut b) = ([0.0; 3], [0.0; 3]);
        for k in 0..self.sn {
            a = add(a, scale(wa[self.sa[k]], self.sl[k]));
            b = add(b, scale(wb[self.sb[k]], self.sl[k]));
        }
        Some((len(self.v), a, b))
    }

    /// Closest point of the simplex to the origin; keep only the vertices that span it. False: origin inside.
    fn reduce(&mut self) -> bool {
        match self.sn {
            1 => {
                self.ri[0] = 0;
                self.rl[0] = 1.0;
                self.rn = 1;
            }
            2 => {
                self.segment(0, 1);
            }
            3 => {
                self.triangle(0, 1, 2);
            }
            _ => {
                if !self.tetrahedron() {
                    return false;
                }
            }
        }
        let (sw, sa, sb) = (self.sw, self.sa, self.sb);
        self.v = [0.0; 3];
        for k in 0..self.rn {
            let i = self.ri[k];
            self.sw[k] = sw[i];
            self.sa[k] = sa[i];
            self.sb[k] = sb[i];
            self.sl[k] = self.rl[k];
            self.v = add(self.v, scale(sw[i], self.rl[k]));
        }
        self.sn = self.rn;
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn result(&mut self, n: usize, i: usize, j: usize, k: usize, li: f64, lj: f64, lk: f64) -> f64 {
        self.rn = n;
        self.ri = [i, j, k];
        self.rl = [li, lj, lk];
        let p = add(
            add(scale(self.sw[i], li), scale(self.sw[j], lj)),
            scale(self.sw[k], lk),
        );
        dot(p, p)
    }

    fn segment(&mut self, i: usize, j: usize) -> f64 {
        let a = self.sw[i];
        let e = sub(self.sw[j], a);
        let ee = dot(e, e);
        let t = if ee > 1e-20 { -dot(a, e) / ee } else { 1.0 };
        if t <= 0.0 {
            self.result(1, i, 0, 0, 1.0, 0.0, 0.0)
        } else if t >= 1.0 {
            self.result(1, j, 0, 0, 1.0, 0.0, 0.0)
        } else {
            self.result(2, i, j, 0, 1.0 - t, t, 0.0)
        }
    }

    fn triangle(&mut self, i: usize, j: usize, k: usize) -> f64 {
        let (a, b, c) = (self.sw[i], self.sw[j], self.sw[k]);
        let (ab, ac) = (sub(b, a), sub(c, a));
        let (d1, d2) = (-dot(ab, a), -dot(ac, a));
        if d1 <= 0.0 && d2 <= 0.0 {
            return self.result(1, i, 0, 0, 1.0, 0.0, 0.0);
        }
        let (d3, d4) = (-dot(ab, b), -dot(ac, b));
        if d3 >= 0.0 && d4 <= d3 {
            return self.result(1, j, 0, 0, 1.0, 0.0, 0.0);
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            let v = d1 / (d1 - d3);
            return self.result(2, i, j, 0, 1.0 - v, v, 0.0);
        }
        let (d5, d6) = (-dot(ab, c), -dot(ac, c));
        if d6 >= 0.0 && d5 <= d6 {
            return self.result(1, k, 0, 0, 1.0, 0.0, 0.0);
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            let w = d2 / (d2 - d6);
            return self.result(2, i, k, 0, 1.0 - w, w, 0.0);
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
            return self.result(2, j, k, 0, 1.0 - w, w, 0.0);
        }
        let den = 1.0 / (va + vb + vc);
        let (v, w) = (vb * den, vc * den);
        self.result(3, i, j, k, 1.0 - v - w, v, w)
    }

    /// Is the origin on the far side of plane (i, j, k) from vertex l? Degenerate tetrahedra count as outside.
    fn outside(&self, i: usize, j: usize, k: usize, l: usize) -> bool {
        let a = self.sw[i];
        let n = cross(sub(self.sw[j], a), sub(self.sw[k], a));
        let sp = -dot(a, n);
        let sd = dot(sub(self.sw[l], a), n);
        sp * sd <= 0.0
    }

    fn tetrahedron(&mut self) -> bool {
        let mut least = f64::INFINITY;
        let (mut bn, mut bi, mut bl) = (0, [0; 3], [0.0; 3]);
        for [i, j, k, l] in TET {
            if !self.outside(i, j, k, l) {
                continue;
            }
            let d = self.triangle(i, j, k);
            if d < least {
                least = d;
                bn = self.rn;
                bi = self.ri;
                bl = self.rl;
            }
        }
        if least == f64::INFINITY {
            return false;
        }
        self.rn = bn;
        self.ri = bi;
        self.rl = bl;
        true
    }
}

/// The solver: contact/group pools and scratch, reused every substep.
#[derive(Default)]
struct Sim {
    contacts: Vec<Contact>,
    nc: usize,
    groups: Vec<Group>,
    ng: usize,
    time: f64,
    gjk: Gjk,
    ca: Vec<V>,
    cb: Vec<V>,
    /// (die vs die, closing speed) of new hard contacts.
    impacts: Vec<(bool, f32)>,
}

impl Sim {
    /// Advance active bodies by dt seconds (fixed substeps). Held bodies chase their target with their spin.
    fn step(&mut self, bodies: &mut [Body], dt: f64, bounds: &Bounds, gravity: V) {
        if dt > 0.0 {
            let dt = dt.min(0.05);
            let n = (dt / SUBSTEP - 1e-9).ceil() as usize;
            for _ in 0..n {
                self.substep(bodies, dt / n as f64, bounds, gravity);
            }
        }
        for b in bodies.iter_mut() {
            if b.active && (!b.sleeping || b.held.is_some()) {
                b.place();
            }
        }
    }

    fn substep(&mut self, bodies: &mut [Body], dt: f64, bounds: &Bounds, gravity: V) {
        self.time += dt;
        let g = scale(gravity, GRAVITY * dt);
        for b in bodies.iter_mut().filter(|b| b.active) {
            if let Some(h) = &b.held {
                b.sleeping = false;
                b.sleep_timer = 0.0;
                let mut v = scale(sub(h.target, b.pos), HOLD_GAIN);
                let s = len(v);
                if s > HOLD_MAX {
                    v = scale(v, HOLD_MAX / s);
                }
                b.vel = v;
                b.ang = h.ang;
            } else if !b.sleeping {
                b.vel = add(b.vel, g);
            }
            b.im = if b.sleeping { 0.0 } else { 1.0 };
            b.ii = if b.sleeping || b.held.is_some() || b.guided {
                0.0
            } else {
                b.inv_inertia
            };
            b.touch = false;
            b.place();
        }

        self.nc = 0;
        self.ng = 0;
        // ponytail: all-pairs bounding-sphere broadphase, 66 pairs at 12 dice; sweep-and-prune if counts grow.
        for i in 0..bodies.len() {
            if !bodies[i].active {
                continue;
            }
            for j in i + 1..bodies.len() {
                let (a, b) = (&bodies[i], &bodies[j]);
                if !b.active || (a.sleeping && b.sleeping) {
                    continue;
                }
                let d = sub(b.pos, a.pos);
                let r = a.solid.radius() + b.solid.radius() + SPECULATIVE;
                if dot(d, d) < r * r {
                    self.collide(bodies, i, j, dt);
                }
            }
        }
        for i in 0..bodies.len() {
            if bodies[i].active && !bodies[i].sleeping {
                self.planes(bodies, i, bounds, dt);
            }
        }

        for _ in 0..ITERATIONS {
            for c in &mut self.contacts[..self.nc] {
                solve(c, bodies);
            }
        }

        for b in bodies.iter_mut() {
            if b.active && !b.sleeping {
                b.pos = add(b.pos, scale(b.vel, dt));
                spin(&mut b.quat, b.ang, dt);
            }
        }

        // Push overlapping bodies apart by most of the penetration (positions only, adds no energy).
        for g in &self.groups[..self.ng] {
            let ia = g.a.map_or(0.0, |a| bodies[a].im);
            let ib = bodies[g.b].im;
            let total = ia + ib;
            if total == 0.0 {
                continue;
            }
            let k = 0.8 * (g.depth - SLOP) / total;
            bodies[g.b].pos = add(bodies[g.b].pos, scale(g.n, k * ib));
            if let Some(a) = g.a {
                bodies[a].pos = sub(bodies[a].pos, scale(g.n, k * ia));
            }
        }

        for i in 0..bodies.len() {
            let b = &mut bodies[i];
            if !b.active || b.sleeping || b.held.is_some() {
                continue;
            }
            if b.guided && b.touch && b.vel[1].abs() < SLEEP_SPEED {
                b.guided = false;
            }
            if dot(b.vel, b.vel) < SLEEP_SPEED * SLEEP_SPEED
                && dot(b.ang, b.ang) < SLEEP_SPIN * SLEEP_SPIN
            {
                b.sleep_timer += dt;
                if b.sleep_timer >= SLEEP_TIME {
                    b.stop();
                    let mut rest = std::mem::take(&mut b.rest);
                    rest.clear();
                    for c in &self.contacts[..self.nc] {
                        let other = if c.b == i {
                            c.a
                        } else if c.a == Some(i) {
                            Some(c.b)
                        } else {
                            None
                        };
                        if let Some(o) = other {
                            let id = bodies[o].id;
                            if !rest.contains(&id) {
                                rest.push(id);
                            }
                        }
                    }
                    bodies[i].rest = rest;
                }
            } else {
                b.sleep_timer = 0.0;
            }
        }
    }

    /// s: separation of the surfaces at the point (negative = penetrating).
    #[allow(clippy::too_many_arguments)]
    fn add_contact(
        &mut self,
        bodies: &mut [Body],
        a: Option<usize>,
        b: usize,
        p: V,
        n: V,
        s: f64,
        material: &Material,
        dt: f64,
    ) {
        let ra = a.map_or([0.0; 3], |a| sub(p, bodies[a].pos));
        let rb = sub(p, bodies[b].pos);
        let [nx, ny, nz] = n;
        let t1 = if nx.abs() > 0.57 {
            let l = nx.hypot(ny);
            [ny / l, -nx / l, 0.0]
        } else {
            let l = ny.hypot(nz);
            [0.0, nz / l, -ny / l]
        };
        let mut c = Contact {
            a,
            b,
            n,
            mu: material.mu,
            ra,
            rb,
            t1,
            t2: cross(n, t1),
            ..Contact::default()
        };
        c.kn = mass(&c, bodies, n);
        c.kt1 = mass(&c, bodies, c.t1);
        c.kt2 = mass(&c, bodies, c.t2);
        let vn = dot(rel_vel(&c, bodies), n);
        // One impact per new hard contact, before any impulse: the real closing speed of both bodies. A short
        // per-body cooldown makes a manifold of several points one sound.
        if vn < -3.0 {
            match a {
                None => {
                    if self.time - bodies[b].last_impact > 0.045 {
                        bodies[b].last_impact = self.time;
                        self.impacts.push((false, -vn as f32));
                    }
                }
                Some(a) => {
                    if self.time - bodies[a].last_clack.max(bodies[b].last_clack) > 0.04 {
                        bodies[a].last_clack = self.time;
                        bodies[b].last_clack = self.time;
                        self.impacts.push((true, -vn as f32));
                    }
                }
            }
        }
        // Impacts reaching contact within this substep bounce; otherwise close the gap without bouncing.
        c.target = if vn < -BOUNCE && s + vn * dt < 0.0 {
            -material.e * vn
        } else if s > 0.0 {
            -s / dt
        } else {
            0.0
        };
        if self.nc == self.contacts.len() {
            self.contacts.push(c);
        } else {
            self.contacts[self.nc] = c;
        }
        self.nc += 1;
    }

    fn add_group(&mut self, a: Option<usize>, b: usize, n: V, depth: f64) {
        if depth <= SLOP {
            return;
        }
        let g = Group { a, b, n, depth };
        if self.ng == self.groups.len() {
            self.groups.push(g);
        } else {
            self.groups[self.ng] = g;
        }
        self.ng += 1;
    }

    /// Core vertices near the table or a wall: every vertex of a resting face is a contact, so dice lie flat.
    fn planes(&mut self, bodies: &mut [Body], i: usize, bounds: &Bounds, dt: f64) {
        let r = bodies[i].solid.margin();
        let mut d = [0.0f64; 5];
        for k in 0..bodies[i].world.len() {
            let [x, y, z] = bodies[i].world[k];
            let s = y - r;
            if s < SPECULATIVE {
                self.add_contact(
                    bodies,
                    None,
                    i,
                    [x, y - r, z],
                    [0.0, 1.0, 0.0],
                    s,
                    &TABLE,
                    dt,
                );
                bodies[i].touch = true;
                d[0] = d[0].max(-s);
            }
            let s = bounds.x - x - r;
            if s < SPECULATIVE {
                self.add_contact(
                    bodies,
                    None,
                    i,
                    [x + r, y, z],
                    [-1.0, 0.0, 0.0],
                    s,
                    &WALL,
                    dt,
                );
                d[1] = d[1].max(-s);
            }
            let s = bounds.x + x - r;
            if s < SPECULATIVE {
                self.add_contact(
                    bodies,
                    None,
                    i,
                    [x - r, y, z],
                    [1.0, 0.0, 0.0],
                    s,
                    &WALL,
                    dt,
                );
                d[2] = d[2].max(-s);
            }
            let s = bounds.max_z - z - r;
            if s < SPECULATIVE {
                self.add_contact(
                    bodies,
                    None,
                    i,
                    [x, y, z + r],
                    [0.0, 0.0, -1.0],
                    s,
                    &WALL,
                    dt,
                );
                d[3] = d[3].max(-s);
            }
            let s = z - bounds.min_z - r;
            if s < SPECULATIVE {
                self.add_contact(
                    bodies,
                    None,
                    i,
                    [x, y, z - r],
                    [0.0, 0.0, 1.0],
                    s,
                    &WALL,
                    dt,
                );
                d[4] = d[4].max(-s);
            }
        }
        self.add_group(None, i, [0.0, 1.0, 0.0], d[0]);
        self.add_group(None, i, [-1.0, 0.0, 0.0], d[1]);
        self.add_group(None, i, [1.0, 0.0, 0.0], d[2]);
        self.add_group(None, i, [0.0, 0.0, -1.0], d[3]);
        self.add_group(None, i, [0.0, 0.0, 1.0], d[4]);
    }

    // ---- die against die ----
    fn collide(&mut self, bodies: &mut [Body], ia: usize, ib: usize, dt: f64) {
        let (ra, rb) = (bodies[ia].solid.margin(), bodies[ib].solid.margin());
        let hit = self.gjk.distance(
            &bodies[ia].world,
            &bodies[ib].world,
            bodies[ia].pos,
            bodies[ib].pos,
        );
        if let Some((d, ..)) = hit {
            if d - ra - rb >= SPECULATIVE {
                return;
            }
        }
        if bodies[ia].sleeping && bodies[ib].moving() {
            bodies[ia].rouse();
        } else if bodies[ib].sleeping && bodies[ia].moving() {
            bodies[ib].rouse();
        }
        if let Some((d, pa, pb)) = hit {
            let n = scale(sub(pb, pa), 1.0 / d);
            // A face of either core lying along the normal carries a multi-point manifold (stable stacking);
            // the GJK point is added when the clipped face misses the closest feature.
            let (fa, da) = bodies[ia].best_face(n);
            let (fb, db) = bodies[ib].best_face(scale(n, -1.0));
            let s = d - ra - rb;
            let (made, closest) = if da.max(db) > 0.9 {
                if da >= db {
                    self.manifold(bodies, ia, fa, ib, true, dt)
                } else {
                    self.manifold(bodies, ib, fb, ia, false, dt)
                }
            } else {
                (0, f64::INFINITY)
            };
            if made > 0 && closest < s + 0.005 {
                return;
            }
            let p = add(scale(add(pa, pb), 0.5), scale(n, (ra - rb) / 2.0));
            self.add_contact(bodies, Some(ia), ib, p, n, s, &DIE, dt);
            self.add_group(Some(ia), ib, n, -s);
            return;
        }
        // Cores overlap (a very fast impact): least-penetration face axis, then the same clipping.
        let (depth_a, face_a) = scan(&bodies[ia], &bodies[ib]);
        let (depth_b, face_b) = scan(&bodies[ib], &bodies[ia]);
        let (made, _) = if depth_b < depth_a {
            self.manifold(bodies, ib, face_b, ia, false, dt)
        } else {
            self.manifold(bodies, ia, face_a, ib, true, dt)
        };
        if made > 0 {
            return;
        }
        // Nothing clipped (degenerate overlap): push apart along the centers.
        let d = sub(bodies[ib].pos, bodies[ia].pos);
        let l = match len(d) {
            l if l > 0.0 => l,
            _ => 1.0,
        };
        let n = scale(d, 1.0 / l);
        self.add_contact(
            bodies,
            Some(ia),
            ib,
            add(bodies[ia].pos, scale(d, 0.5)),
            n,
            -SLOP,
            &DIE,
            dt,
        );
        self.add_group(Some(ia), ib, n, 2.0 * SLOP);
    }

    /// Contacts of the other body's most opposed face clipped to the reference face's prism. Returns their
    /// count and the smallest separation among them.
    #[allow(clippy::too_many_arguments)]
    fn manifold(
        &mut self,
        bodies: &mut [Body],
        r: usize,
        fi: usize,
        inc: usize,
        ref_is_a: bool,
        dt: f64,
    ) -> (usize, f64) {
        let (a, b, sg) = if ref_is_a {
            (r, inc, 1.0)
        } else {
            (inc, r, -1.0)
        };
        let (n, h) = {
            let (rb, ib) = (&bodies[r], &bodies[inc]);
            let ring = &rb.solid.shape.faces[fi].idx;
            let n = rotate(&rb.m, rb.solid.normals[fi]);
            let h = dot(rb.world[ring[0]], n);
            let (incident, _) = ib.best_face(scale(n, -1.0));
            self.ca.clear();
            self.ca.extend(
                ib.solid.shape.faces[incident]
                    .idx
                    .iter()
                    .map(|&k| ib.world[k]),
            );
            for k in 0..ring.len() {
                if self.ca.is_empty() {
                    break;
                }
                let p = rb.world[ring[k]];
                let side = cross(sub(rb.world[ring[(k + 1) % ring.len()]], p), n); // points out of the face
                clip_poly(&self.ca, &mut self.cb, side, dot(side, p));
                std::mem::swap(&mut self.ca, &mut self.cb);
            }
            (n, h)
        };
        let rr = bodies[r].solid.margin() + bodies[inc].solid.margin();
        let (mut made, mut deepest, mut closest) = (0, 0.0f64, f64::INFINITY);
        for k in 0..self.ca.len() {
            let x = self.ca[k];
            let gap = dot(x, n) - h;
            let s = gap - rr;
            if s >= SPECULATIVE {
                continue;
            }
            self.add_contact(
                bodies,
                Some(a),
                b,
                sub(x, scale(n, gap / 2.0)),
                scale(n, sg),
                s,
                &DIE,
                dt,
            );
            made += 1;
            deepest = deepest.max(-s);
            closest = closest.min(s);
        }
        if made > 0 {
            self.add_group(Some(a), b, scale(n, sg), deepest);
        }
        (made, closest)
    }
}

struct Grip {
    id: u32,
    x: f64,
    z: f64,
    pos: V,
    quat: Q,
    vel: V,
    ang: V,
    sleeping: bool,
    tray: Option<String>,
    guided: bool,
    randomize: bool,
    vx: f64,
    vz: f64,
    last_motion: f64,
    wobble_limit: f64,
    rotated: bool,
}

/// The dice table: bodies for every die (tray dice rest outside the simulation), grip, tilt and throws.
/// Simulation stops completely at rest: `step` returns false and does nothing until something moves.
pub struct PhysicsWorld {
    solids: Vec<Arc<Solid>>,
    bodies: Vec<Body>,
    sim: Sim,
    width: f64,
    height: f64,
    bounds: Bounds,
    gravity: V,
    grip: Option<Grip>,
    accumulator: f64,
    /// (die vs die, closing speed) of hard impacts; true = die against die, false = table or wall. Drain it.
    pub impacts: Vec<(bool, f32)>,
    /// (die id, triggered) like onFlickRandomize: false on a non-gentle release, true once a tumbling
    /// flick really renumbered the die (refresh its face_values). Drain it.
    pub randomized: Vec<(u32, bool)>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        PhysicsWorld {
            solids: Vec::new(),
            bodies: Vec::new(),
            sim: Sim::default(),
            width: 6.0,
            height: 8.0,
            bounds: Bounds {
                x: 3.0,
                z: 4.0,
                min_z: -4.0,
                max_z: 4.0,
            },
            gravity: [0.0, -1.0, 0.0],
            grip: None,
            accumulator: 0.0,
            impacts: Vec::new(),
            randomized: Vec::new(),
        }
    }

    fn solid(&mut self, sides: u32) -> Arc<Solid> {
        if let Some(s) = self.solids.iter().find(|s| s.shape.sides == sides) {
            return s.clone();
        }
        let s = Arc::new(Solid::new(sides));
        self.solids.push(s.clone());
        s
    }

    fn index(&self, id: u32) -> Option<usize> {
        self.bodies.iter().position(|b| b.id == id)
    }

    /// Mirror the model: create/remove bodies, restore stored values of new dice and dice moving between
    /// tray and table (a die returning to the table takes the freest spot). width/height: full nominal
    /// table in world units; a size change scales positions like a layout change. Call after `end_grip`.
    pub fn sync(&mut self, dice: &[Die], width: f32, height: f32) {
        self.bodies.retain(|b| dice.iter().any(|d| d.id == b.id));
        if self
            .grip
            .as_ref()
            .is_some_and(|g| !self.bodies.iter().any(|b| b.id == g.id))
        {
            self.grip = None;
        }
        let (width, height) = (width as f64, height as f64);
        if width > 0.0 && height > 0.0 && (width != self.width || height != self.height) {
            let k = height / self.height;
            let (min_z, max_z) = (self.bounds.min_z * k, self.bounds.max_z * k);
            self.width = width;
            self.height = height;
            self.relayout(width / 2.0, height / 2.0, min_z, max_z);
        }
        for d in dice {
            if let Some(i) = self.index(d.id) {
                self.bodies[i].in_tray = d.tray.is_some();
            }
        }
        let count = dice.iter().filter(|d| d.tray.is_none()).count();
        let columns = ((count as f64 * self.width / self.height).sqrt().ceil() as usize).max(1);
        let rows = count.div_ceil(columns);
        let grip = self.grip.as_ref().map(|g| g.id);
        let mut index = 0;
        for d in dice {
            let i = match self.index(d.id) {
                Some(i) if self.bodies[i].solid.shape.sides == d.sides => i,
                found => {
                    let x = ((index % columns) as f64 - (columns as f64 - 1.0) / 2.0) * 1.4;
                    let z = ((index / columns) as f64 - (rows as f64 - 1.0) / 2.0) * 1.4;
                    let mut body = Body::new(d.id, self.solid(d.sides), [x, 1.0, z]);
                    body.restore_value(d.value);
                    body.sleeping = true;
                    body.in_tray = d.tray.is_some();
                    match found {
                        Some(i) => {
                            self.bodies[i] = body;
                            i
                        }
                        None => {
                            self.bodies.push(body);
                            self.bodies.len() - 1
                        }
                    }
                }
            };
            // A tray is storage, not another roll: the stored result stays; the held die settles in end_grip.
            if self.bodies[i].tray != d.tray && grip != Some(d.id) {
                if d.tray.is_none() {
                    self.place_on_table(i);
                }
                let b = &mut self.bodies[i];
                b.tray = d.tray.clone();
                b.restore_value(d.value);
                b.stop();
            }
            if d.tray.is_none() {
                index += 1;
            }
        }
    }

    /// Empty tray space extends the playable table: walls at z = min_z and max_z (defaults ±height/2).
    pub fn set_play_bounds(&mut self, min_z: f32, max_z: f32) {
        let (x, z) = (self.bounds.x, self.bounds.z);
        self.relayout(x, z, min_z as f64, max_z as f64);
    }

    /// New walls. Resizing scales table positions; only a shrinking wall moves a die, by its actual
    /// rotated shape, not its bounding sphere.
    fn relayout(&mut self, x: f64, z: f64, min_z: f64, max_z: f64) {
        let old = self.bounds;
        self.bounds = Bounds { x, z, min_z, max_z };
        let resize = x != old.x || z != old.z;
        let shrink_x = x < old.x;
        let shrink_z = min_z > old.min_z || max_z < old.max_z;
        for b in self.bodies.iter_mut().filter(|b| !b.in_tray) {
            if resize {
                b.pos[0] *= x / old.x;
                b.pos[2] *= z / old.z;
            }
            if shrink_x || shrink_z {
                let m = mat3(b.quat);
                let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                for &v in &b.solid.verts {
                    let w = rotate(&m, v);
                    lo = [lo[0].min(w[0]), lo[1].min(w[2])];
                    hi = [hi[0].max(w[0]), hi[1].max(w[2])];
                }
                let margin = b.solid.margin();
                if shrink_x {
                    b.pos[0] = clamp(b.pos[0], -x - lo[0] + margin, x - hi[0] - margin);
                }
                if shrink_z {
                    b.pos[2] = clamp(b.pos[2], min_z - lo[1] + margin, max_z - hi[1] - margin);
                }
            }
            b.place();
        }
    }

    /// The candidate spot with the most clearance from the other table dice, nearest the center on ties.
    fn place_on_table(&mut self, i: usize) {
        let r = self.bodies[i].solid.radius();
        let bounds = self.bounds;
        let x_max = bounds.x - r - 0.1;
        let z_min = bounds.min_z + r + 0.1;
        let z_max = bounds.max_z - r - 0.1;
        let (mut best, mut nearest, mut spot) = (f64::NEG_INFINITY, f64::INFINITY, (0.0, 0.0));
        // ponytail: 25 candidates, at most 12 dice; use packing if the dice limit grows.
        for iz in -2..=2 {
            for ix in -2..=2 {
                let x = ix as f64 * x_max / 2.0;
                let z = (z_min + z_max) / 2.0 + iz as f64 * (z_max - z_min) / 4.0;
                let mut clearance = f64::INFINITY;
                for (j, o) in self.bodies.iter().enumerate() {
                    if j == i || o.tray.is_some() || o.in_tray {
                        continue;
                    }
                    clearance =
                        clearance.min((x - o.pos[0]).hypot(z - o.pos[2]) - r - o.solid.radius());
                }
                let distance = x * x + z * z;
                if clearance > best || (clearance == best && distance < nearest) {
                    best = clearance;
                    nearest = distance;
                    spot = (x, z);
                }
            }
        }
        self.bodies[i].pos[0] = spot.0;
        self.bodies[i].pos[2] = spot.1;
    }

    /// Throw these dice. Every throw first renumbers each die with fresh cryptographic face values (opposite
    /// numbers stay opposite), so each physical face is uniform even on the unequal d100 faces. Calm (reduced
    /// motion): a cryptographic value appears instantly, without motion.
    pub fn throw(&mut self, ids: &[u32], calm: bool) {
        self.throw_with(ids, calm, &mut crypto_unit);
    }

    fn throw_with(&mut self, ids: &[u32], calm: bool, random_unit: &mut dyn FnMut() -> f64) {
        let n = ids.len();
        let columns = ((n as f64 * self.width / self.height).sqrt().ceil() as usize).max(1);
        let rows = n.div_ceil(columns);
        for (i, &id) in ids.iter().enumerate() {
            let Some(bi) = self.index(id) else { continue };
            let b = &mut self.bodies[bi];
            b.renumber();
            b.guided = false;
            b.randomize_on_spin = false;
            if calm {
                b.restore_value(random(b.solid.shape.sides));
                continue;
            }
            b.quat = random_quat(random_unit);
            let (col, row) = ((i % columns) as f64, (i / columns) as f64);
            b.pos = [
                (col - (columns as f64 - 1.0) / 2.0) * 1.25,
                2.5 + row * 1.2 + random_unit() * 0.4,
                (row - (rows as f64 - 1.0) / 2.0) * 1.25,
            ];
            b.vel = [
                (random_unit() - 0.5) * 4.0,
                2.0 + random_unit() * 3.0,
                (random_unit() - 0.5) * 4.0,
            ];
            b.ang = [
                (random_unit() - 0.5) * 24.0,
                (random_unit() - 0.5) * 24.0,
                (random_unit() - 0.5) * 24.0,
            ];
            b.sleeping = false;
            b.sleep_timer = 0.0;
            b.place();
        }
    }

    /// One animation frame of dt seconds (fixed 1/120 s physics steps). Returns whether anything still
    /// moves; at rest nothing is simulated and no frame needs drawing.
    pub fn step(&mut self, dt: f32) -> bool {
        let dt = if dt.is_finite() {
            (dt as f64).clamp(0.0, 0.05)
        } else {
            0.0
        };
        let grip = self.grip.as_ref().and_then(|g| self.index(g.id));
        for (i, b) in self.bodies.iter_mut().enumerate() {
            b.active = !b.in_tray || Some(i) == grip;
        }
        // Check support before the idle gate: otherwise a sleeping stack never wakes when its base is removed.
        for i in 0..self.bodies.len() {
            let b = &self.bodies[i];
            if b.active
                && b.sleeping
                && b.rest
                    .iter()
                    .any(|&id| !self.bodies.iter().any(|o| o.active && o.id == id))
            {
                self.bodies[i].rouse();
            }
        }
        if let (Some(g), Some(i)) = (&self.grip, grip) {
            let b = &mut self.bodies[i];
            if let Some(h) = &mut b.held {
                // A finger spin coasts and slows gradually, so the die can be dropped while still spinning.
                h.ang = scale(h.ang, (-SPIN_DECAY * dt).exp());
                if !g.rotated {
                    let blend = 1.0 - (-12.0 * dt).exp();
                    for c in 0..4 {
                        b.quat[c] += (h.quat[c] - b.quat[c]) * blend;
                    }
                    normalize_q(&mut b.quat);
                }
            }
        }
        // Sample release spin before contacts dissipate it.
        for b in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && b.randomize_on_spin)
        {
            if b.ang[0].hypot(b.ang[2]) >= RANDOMIZE_SPIN {
                b.renumber();
                b.randomize_on_spin = false; // once per throw; yaw alone leaves the upper face readable
                self.randomized.push((b.id, true));
            } else if b.sleeping {
                b.randomize_on_spin = false;
            }
        }
        let moving = |bodies: &[Body]| {
            bodies
                .iter()
                .any(|b| b.active && (!b.sleeping || b.held.is_some()))
        };
        let mut active = moving(&self.bodies);
        if active {
            self.accumulator += dt;
            while self.accumulator >= FRAME {
                self.sim
                    .step(&mut self.bodies, FRAME, &self.bounds, self.gravity);
                self.accumulator -= FRAME;
            }
            self.impacts.append(&mut self.sim.impacts);
            active = moving(&self.bodies);
        }
        if !active {
            self.accumulator = 0.0;
        }
        active
    }

    /// Poses of the dice drawn on the table: table dice plus a held die over the play area.
    pub fn poses(&self) -> Vec<Pose> {
        let mut out = Vec::with_capacity(self.bodies.len());
        self.write_poses(&mut out);
        out
    }

    pub fn write_poses(&self, out: &mut Vec<Pose>) {
        out.clear();
        let b = self.bounds;
        let grip = self
            .grip
            .as_ref()
            .map(|g| (g.id, g.x.abs() <= b.x && g.z >= b.min_z && g.z <= b.max_z));
        for body in &self.bodies {
            let shown = match grip {
                Some((id, inside)) if id == body.id => inside,
                _ => !body.in_tray,
            };
            if shown {
                out.push(body.pose());
            }
        }
    }

    /// (id, shown value) of every table die not in hand: commit them once `step` returns false after a
    /// throw, a drop or tilting.
    pub fn values(&self) -> Vec<(u32, u32)> {
        self.bodies
            .iter()
            .filter(|b| !b.in_tray && b.held.is_none())
            .map(|b| (b.id, b.displayed()))
            .collect()
    }

    /// Pose of the held die wherever it is (also over a tray or outside the table), for previews.
    pub fn grip_pose(&self) -> Option<Pose> {
        let id = self.grip.as_ref()?.id;
        self.index(id).map(|i| self.bodies[i].pose())
    }

    /// Current numbering: face_values[k - 1] is printed on geometric face value k.
    pub fn face_values(&self, id: u32) -> Option<&[u32]> {
        self.index(id)
            .map(|i| self.bodies[i].face_values.as_slice())
    }

    /// Tilt: horizontal components of the unit gravity direction (vertical follows). Wakes free table dice
    /// and never rerolls them (sensor play is not a cryptographic throw).
    pub fn set_tilt(&mut self, x: f32, z: f32) {
        if !x.is_finite() || !z.is_finite() {
            return;
        }
        let (x, z) = (x as f64, z as f64);
        let g = [x, -(1.0 - x * x - z * z).max(0.0).sqrt(), z];
        if len(sub(g, self.gravity)) < TILT_DEADBAND {
            return;
        }
        self.gravity = g;
        for b in self
            .bodies
            .iter_mut()
            .filter(|b| !b.in_tray && b.held.is_none())
        {
            b.sleeping = false;
            b.sleep_timer = 0.0;
            b.guided = false;
            b.randomize_on_spin = false;
        }
    }

    /// Nearest table die whose hull the ray hits (origin and direction in world space).
    pub fn pick(&self, origin: [f32; 3], dir: [f32; 3]) -> Option<u32> {
        let o3 = [origin[0] as f64, origin[1] as f64, origin[2] as f64];
        let d3 = [dir[0] as f64, dir[1] as f64, dir[2] as f64];
        let grip = self.grip.as_ref().map(|g| g.id);
        let mut best = (f64::INFINITY, None);
        for b in self
            .bodies
            .iter()
            .filter(|b| !b.in_tray && Some(b.id) != grip)
        {
            let m = mat3(b.quat);
            let (o, d) = (unrotate(&m, sub(o3, b.pos)), unrotate(&m, d3));
            let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
            for (&n, &dist) in b.solid.normals.iter().zip(&b.solid.planes) {
                let denom = dot(n, d);
                let num = dist - dot(n, o);
                if denom.abs() < 1e-9 {
                    if num < 0.0 {
                        t0 = f64::INFINITY;
                        break;
                    }
                    continue;
                }
                let t = num / denom;
                if denom < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
            if t0 <= t1 && t0 < best.0 {
                best = (t0, Some(b.id));
            }
        }
        best.1
    }

    /// Pick up a die (table or tray) at world point (x, z) under the pointer.
    pub fn begin_grip(&mut self, id: u32, x: f32, z: f32) {
        if self.grip.is_some() {
            self.end_grip(true);
        }
        let Some(i) = self.index(id) else { return };
        let time = now();
        let b = &mut self.bodies[i];
        // Stay inside the current face's angular region, including tiny D100 faces.
        let shape = &b.solid.shape;
        let m = mat3(b.quat);
        let value = read_value(shape, b.rotation());
        let up = shape
            .faces
            .iter()
            .position(|f| f.value == value)
            .unwrap_or(0);
        let n = b.solid.normals[up];
        let sign = if shape.bottom_read { -1.0 } else { 1.0 };
        let mut margin = 1.0f64;
        for (k, &f) in b.solid.normals.iter().enumerate() {
            if k != up {
                let d = sub(n, f);
                margin = margin.min(sign * rotate(&m, d)[1] / len(d));
            }
        }
        self.grip = Some(Grip {
            id,
            x: x as f64,
            z: z as f64,
            pos: b.pos,
            quat: b.quat,
            vel: b.vel,
            ang: b.ang,
            sleeping: b.sleeping,
            tray: b.tray.clone(),
            guided: b.guided,
            randomize: b.randomize_on_spin,
            vx: 0.0,
            vz: 0.0,
            last_motion: time,
            wobble_limit: 0.08f64.min(clamp(margin, 0.0, 1.0).asin() / 2.0),
            rotated: false,
        });
        b.sleeping = false;
        b.sleep_timer = 0.0;
        b.guided = false;
        b.randomize_on_spin = false;
        b.held = Some(Held {
            target: b.pos,
            ang: [0.0; 3],
            quat: b.quat,
        });
        self.move_grip(x, z, 0.0, 0.0, false);
        if let Some(g) = &mut self.grip {
            g.last_motion = time;
        }
    }

    /// Move the held die toward world point (x, z), or with `rotate` spin it by the pointer delta
    /// (dx, dy in CSS pixels): faster strokes speed the spin up, a reverse stroke brakes by its own size.
    pub fn move_grip(&mut self, x: f32, z: f32, dx: f32, dy: f32, rotate: bool) {
        let Some(g) = &mut self.grip else { return };
        let Some(i) = self.bodies.iter().position(|b| b.id == g.id) else {
            return;
        };
        let b = &mut self.bodies[i];
        let Some(held) = &mut b.held else { return };
        let time = now();
        let dt = (time - g.last_motion).max(1.0 / 120.0);
        g.last_motion = time;
        let (dx, dy) = (dx as f64, dy as f64);
        let moved = dx != 0.0 || dy != 0.0;
        if !rotate {
            let (x, z) = (x as f64, z as f64);
            g.x = x;
            g.z = z;
            let r = b.solid.radius();
            let bounds = self.bounds;
            let target = [
                clamp(x, -bounds.x + r, bounds.x - r),
                HELD_HEIGHT,
                clamp(z, bounds.min_z + r, bounds.max_z - r),
            ];
            g.vx = if moved {
                (target[0] - held.target[0]) / dt
            } else {
                0.0
            };
            g.vz = if moved {
                (target[2] - held.target[2]) / dt
            } else {
                0.0
            };
            held.target = target;
            if !g.rotated {
                // Hand motion tilts the held die a little toward the motion, never off its face.
                let speed = g.vx.hypot(g.vz);
                let angle = g.wobble_limit * speed / (speed + MOVE_SPEED);
                let s = if speed > 0.0 {
                    (angle / 2.0).sin() / speed
                } else {
                    0.0
                };
                let c = (angle / 2.0).cos();
                let (x, z) = (g.vz * s, -g.vx * s);
                let [bx, by, bz, bw] = g.quat;
                held.quat = [
                    c * bx + x * bw - z * by,
                    c * by + z * bx - x * bz,
                    c * bz + z * bw + x * by,
                    c * bw - x * bx - z * bz,
                ];
            }
        } else {
            g.vx = 0.0;
            g.vz = 0.0;
            if moved {
                g.rotated = true;
            }
            let push = |cur: f64, next: f64| {
                if next * cur < 0.0 {
                    cur + next
                } else if next.abs() > cur.abs() {
                    next
                } else {
                    cur
                }
            };
            held.ang[0] = push(held.ang[0], clamp(dy * 0.3, -24.0, 24.0));
            held.ang[1] = push(held.ang[1], clamp(dx * 0.3, -24.0, 24.0));
        }
    }

    /// Release the held die. cancel: back exactly as before pickup (tray included). Otherwise it drops onto
    /// the table where it is: gentle moves lower it without changing its face, flicks throw it with
    /// tumbling spin, a finger spin keeps spinning. A die dropped into a tray: `sync` right after restores
    /// its stored result.
    pub fn end_grip(&mut self, cancel: bool) {
        let Some(g) = self.grip.take() else { return };
        let Some(i) = self.index(g.id) else { return };
        let time = now();
        let b = &mut self.bodies[i];
        let held_ang = b.held.take().map(|h| h.ang);
        if cancel {
            b.pos = g.pos;
            b.quat = g.quat;
            b.vel = g.vel;
            b.ang = g.ang;
            b.sleeping = g.sleeping;
            b.guided = g.guided;
            b.randomize_on_spin = g.randomize;
            b.tray = g.tray;
            b.place();
            return;
        }
        b.tray = None;
        let stale = time - g.last_motion > 0.08;
        let (vx, vz) = if stale { (0.0, 0.0) } else { (g.vx, g.vz) };
        let speed = vx.hypot(vz);
        let t = clamp((speed - MOVE_SPEED) / (FLICK_SPEED - MOVE_SPEED), 0.0, 1.0);
        let blend = t * t * (3.0 - 2.0 * t); // continuous speed and spin, including at the threshold
        b.vel[0] = clamp(vx, -8.0, 8.0) * blend;
        b.vel[2] = clamp(vz, -8.0, 8.0) * blend;
        b.vel[1] = b.vel[1].min(0.0); // hand lift is not an upward throw impulse
        b.guided = blend == 0.0 && !g.rotated;
        b.randomize_on_spin = !b.guided;
        if b.guided {
            // Lower naturally, without changing the face or adding sideways momentum.
            b.vel[0] = 0.0;
            b.vel[2] = 0.0;
            b.ang = [0.0; 3];
        } else if !g.rotated {
            let spin = MAX_FLICK_SPIN * blend / speed;
            b.ang = [vz * spin, 0.0, -vx * spin];
        } else if let Some(a) = held_ang {
            b.ang = a;
        }
        b.sleeping = false;
        b.sleep_timer = 0.0;
        self.randomized.push((g.id, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDES: [u32; 7] = [4, 6, 8, 10, 12, 20, 100];

    /// Seeded random so every run is identical.
    fn rng(seed: u32) -> impl FnMut() -> f64 {
        let mut s = seed;
        move || {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            s as f64 / 4_294_967_296.0
        }
    }

    fn die(id: u32, sides: u32, value: u32, tray: Option<&str>) -> Die {
        Die {
            id,
            sides,
            value,
            color: "white".into(),
            numbered: false,
            story: None,
            tray: tray.map(Into::into),
            slot: None,
        }
    }

    /// Height of the lowest point of the rounded die above the table.
    fn lowest(b: &Body) -> f64 {
        let m = mat3(b.quat);
        b.pos[1]
            + b.solid
                .verts
                .iter()
                .map(|&v| rotate(&m, v)[1])
                .fold(f64::INFINITY, f64::min)
            - b.solid.margin()
    }

    /// Cosine between the resting face and the table: 1 when the die lies flat on a face.
    fn flatness(b: &Body) -> f64 {
        let m = mat3(b.quat);
        b.solid
            .normals
            .iter()
            .map(|&n| -rotate(&m, n)[1])
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Throw, then frames until rest (20 s at most), checking every frame.
    fn settle(world: &mut PhysicsWorld, mut check: impl FnMut(&PhysicsWorld)) {
        for _ in 0..1200 {
            let active = world.step(1.0 / 60.0);
            check(world);
            if !active {
                return;
            }
        }
        panic!("dice never settled");
    }

    /// Every die type thrown alone tumbles, never sinks or teleports, lies flat on the table and reports
    /// exactly the printed number the renderer shows (face map of its upper face; d4 apex).
    #[test]
    fn every_die_lands_flat_and_reports_its_rendered_face() {
        for sides in SIDES {
            let mut world = PhysicsWorld::new();
            world.sync(&[die(1, sides, 1, None)], 20.0, 20.0);
            world.throw_with(&[1], false, &mut rng(sides));
            let mut last = world.bodies[0].pos;
            settle(&mut world, |w| {
                let b = &w.bodies[0];
                assert!(lowest(b) > -0.02, "d{sides} sinks into the table");
                assert!(len(sub(b.pos, last)) < 0.5, "d{sides} teleports");
                last = b.pos;
            });
            assert!(!world.step(1.0 / 60.0), "d{sides} stays asleep");
            let b = &world.bodies[0];
            assert!(
                lowest(b).abs() < 0.01,
                "d{sides} rests on the table ({})",
                lowest(b)
            );
            assert!(flatness(b) > 0.999, "d{sides} lies flat ({})", flatness(b));

            let map = world.face_values(1).unwrap().to_vec();
            let shape = &b.solid.shape;
            let mut sorted = map.clone();
            sorted.sort();
            assert_eq!(
                sorted,
                (1..=sides).collect::<Vec<_>>(),
                "d{sides} numbering is a permutation"
            );
            if sides != 4 {
                for f in &shape.faces {
                    let n = Vec3::from(f.normal);
                    let o = shape
                        .faces
                        .iter()
                        .find(|g| n.dot(Vec3::from(g.normal)) < -0.9999)
                        .unwrap();
                    assert_eq!(
                        map[f.value as usize - 1] + map[o.value as usize - 1],
                        sides + 1,
                        "d{sides} opposite pairs"
                    );
                }
            }
            let pose = world.poses()[0];
            assert_eq!(
                world.values(),
                vec![(1, map[read_value(shape, pose.rotation) as usize - 1])]
            );
        }
    }

    /// A dozen mixed dice into a heap: true-shape contacts, nobody sinks into another die or the table,
    /// all stay inside the walls, and the same initial conditions land bit for bit the same.
    #[test]
    fn heap_settles_without_interpenetration_deterministically() {
        let run = || {
            let mut world = PhysicsWorld::new();
            let dice: Vec<Die> = (0..12)
                .map(|i| die(i, SIDES[i as usize % 7], 1, None))
                .collect();
            world.sync(&dice, 6.0, 8.0);
            world.throw_with(&(0..12).collect::<Vec<u32>>(), false, &mut rng(3));
            let (mut deepest, mut floor) = (0.0f64, 0.0f64);
            settle(&mut world, |w| {
                for (i, a) in w.bodies.iter().enumerate() {
                    floor = floor.min(lowest(a));
                    // Each die contains its insphere, so closer centers would mean overlapping dice.
                    for b in &w.bodies[i + 1..] {
                        let r = (a.solid.shape.inradius + b.solid.shape.inradius) as f64;
                        deepest = deepest.max(r - len(sub(a.pos, b.pos)));
                    }
                }
            });
            assert!(deepest < 0.03, "dice interpenetrate ({deepest})");
            assert!(floor > -0.02, "a die sinks into the table ({floor})");
            for b in &world.bodies {
                assert!(b.sleeping && lowest(b) > -0.01);
                assert!(
                    b.pos[0].abs() < 3.0 && b.pos[2].abs() < 4.0,
                    "inside the walls"
                );
            }
            world.poses()
        };
        assert_eq!(run(), run());
    }

    /// Restored results: every value of every die shows on top at rest, also through a tray and back, and a
    /// die on top of another falls once its support goes to a tray.
    #[test]
    fn restored_values_orient_and_tray_round_trips_keep_them() {
        for sides in SIDES {
            let mut world = PhysicsWorld::new();
            for value in 1..=sides {
                world.sync(&[], 6.0, 8.0);
                world.sync(&[die(1, sides, value, None)], 6.0, 8.0);
                assert_eq!(
                    world.values(),
                    vec![(1, value)],
                    "d{sides} restores {value}"
                );
                assert!(
                    lowest(&world.bodies[0]).abs() < 1e-5
                        && flatness(&world.bodies[0]) > 1.0 - 1e-6
                );
                world.sync(&[die(1, sides, value, Some("t1"))], 6.0, 8.0);
                assert!(
                    world.poses().is_empty()
                        && world.values().is_empty()
                        && !world.step(1.0 / 60.0)
                );
                world.sync(&[die(1, sides, value, None)], 6.0, 8.0);
                assert_eq!(
                    world.values(),
                    vec![(1, value)],
                    "d{sides} {value} back from the tray"
                );
            }
        }
        let mut world = PhysicsWorld::new();
        world.sync(&[die(1, 6, 1, None), die(2, 6, 1, None)], 6.0, 8.0);
        let h = world.bodies[0].solid.shape.inradius as f64;
        world.bodies[1].pos = [
            world.bodies[0].pos[0] + 0.1,
            3.0 * h,
            world.bodies[0].pos[2],
        ];
        world.bodies[1].sleeping = false;
        settle(&mut world, |_| {});
        assert!(
            (world.bodies[1].pos[1] - 3.0 * h).abs() < 0.01,
            "stacked die rests on the other"
        );
        world.sync(&[die(1, 6, 1, Some("b1")), die(2, 6, 1, None)], 6.0, 8.0);
        settle(&mut world, |_| {});
        assert!(
            (world.bodies[1].pos[1] - h).abs() < 0.01,
            "falls once its support is gone"
        );
    }
}
