// dice.wgsl — lit, textured dice meshes, their shadow map and the shadow receiver over the CSS table.
// Port of the WebGL2 shaders of renderer3d.js. Geometry uvs use GL's bottom-up v; vs_die flips it.

struct Globals {
  shadow_mat: mat4x4<f32>, // world → shadow map uv (y down) + depth
  light_vp: mat4x4<f32>,
  key_dir: vec4<f32>,
  key_color: vec4<f32>,
  fill_dir: vec4<f32>,
  fill_color: vec4<f32>,
  sky: vec4<f32>,
  flags: vec4<f32>, // x: 1 = sRGB render target (re-linearize the gamma-encoded output)
};

struct Camera {
  view_proj: mat4x4<f32>,
  eye: vec4<f32>,      // xyz camera position, w: 1 = sample the shadow map
  env_map: vec4<f32>,  // table texture uv = xz * x + yz; w = texture size
  ground: vec4<f32>,   // half extent x, half extent z, mode (0 shadow map, 1 analytic sphere), occluder count
  strength: vec4<f32>, // x shadow, y contact occlusion
  occ: array<vec4<f32>, 12>,
};

struct Model {
  model: mat4x4<f32>,
  mat: vec4<f32>,       // roughness, metalness
  highlight: vec4<f32>, // rgb, strength
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var env_tex: texture_2d<f32>;
@group(0) @binding(2) var env_smp: sampler;
@group(0) @binding(3) var shadow_tex: texture_depth_2d;
@group(0) @binding(4) var shadow_smp: sampler_comparison;
@group(1) @binding(0) var<uniform> cam: Camera;
@group(2) @binding(0) var<uniform> m: Model;
@group(3) @binding(0) var atlas_tex: texture_2d<f32>;
@group(3) @binding(1) var atlas_smp: sampler;

const PI: f32 = 3.14159265;

struct DieOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) world: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) shadow: vec3<f32>,
  @location(3) uv: vec2<f32>,
};

@vertex
fn vs_die(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @location(2) uv: vec2<f32>) -> DieOut {
  let w = m.model * vec4<f32>(p, 1.0);
  let nn = (m.model * vec4<f32>(n, 0.0)).xyz;
  var o: DieOut;
  o.pos = cam.view_proj * w;
  o.world = w.xyz;
  o.normal = nn;
  o.shadow = (g.shadow_mat * vec4<f32>(w.xyz + nn * 0.02, 1.0)).xyz;
  o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
  return o;
}

fn shadow_at(s: vec3<f32>) -> f32 {
  if (cam.eye.w < 0.5 || any(s.xy < vec2<f32>(0.0)) || any(s > vec3<f32>(1.0))) {
    return 1.0;
  }
  let t = 1.5 / vec2<f32>(textureDimensions(shadow_tex));
  var sum = 0.0;
  for (var i = -1; i <= 1; i++) {
    for (var j = -1; j <= 1; j++) {
      sum += textureSampleCompareLevel(shadow_tex, shadow_smp, s.xy + vec2<f32>(f32(i), f32(j)) * t, s.z - 0.0015);
    }
  }
  return sum / 9.0;
}

fn light(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, diff: vec3<f32>, F0: vec3<f32>, rough: f32) -> vec3<f32> {
  let NL = dot(N, L);
  if (NL <= 0.0) {
    return vec3<f32>(0.0);
  }
  let H = normalize(V + L);
  let NH = max(dot(N, H), 0.0);
  let NV = max(dot(N, V), 1e-4);
  let VH = max(dot(V, H), 0.0);
  let r = max(rough, 0.18); // the key light is a softbox, not a point
  let a2 = r * r * r * r;
  let d = NH * NH * (a2 - 1.0) + 1.0;
  let k = (r + 1.0) * (r + 1.0) / 8.0;
  let F = F0 + (1.0 - F0) * pow(1.0 - VH, 5.0);
  let spec = a2 / (PI * d * d) * F / (4.0 * (NL * (1.0 - k) + k) * (NV * (1.0 - k) + k));
  return (diff / PI * (1.0 - F) + spec) * NL;
}

// Karis' analytic split-sum approximation.
fn env_brdf(r: f32, NV: f32) -> vec2<f32> {
  let r4 = r * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
  let a = min(r4.x * r4.x, exp2(-9.28 * NV)) * r4.x + r4.y;
  return vec2<f32>(-1.04, 1.04) * a + r4.zw;
}

fn table(xz: vec2<f32>, lod: f32) -> vec3<f32> {
  return textureSampleLevel(env_tex, env_smp, xz * cam.env_map.x + cam.env_map.yz, lod).rgb;
}

fn env(P: vec3<f32>, R: vec3<f32>, r: f32) -> vec3<f32> {
  let sky = g.sky.rgb * (0.55 + 0.7 * max(R.y, 0.0))
    + g.key_color.rgb * 0.06 * pow(max(dot(R, g.key_dir.xyz), 0.0), mix(64.0, 4.0, r));
  let t = max(P.y, 0.0) / max(-R.y, 1e-3);
  let lod = log2(max((0.05 + r * 1.2) * t * cam.env_map.x * cam.env_map.w, 1.0));
  return mix(table(P.xz + R.xz * t, lod) * 0.9, sky, smoothstep(-0.12, 0.08, R.y));
}

fn aces(x: vec3<f32>) -> vec3<f32> {
  return clamp(x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_die(i: DieOut) -> @location(0) vec4<f32> {
  let albedo = textureSample(atlas_tex, atlas_smp, i.uv).rgb;
  let N = normalize(i.normal);
  let V = normalize(cam.eye.xyz - i.world);
  let r = m.mat.x;
  let metal = m.mat.y;
  let NV = max(dot(N, V), 1e-4);
  let F0 = mix(vec3<f32>(0.04), albedo, metal);
  let diff = albedo * (1.0 - metal);
  var col = light(N, V, g.key_dir.xyz, diff, F0, r) * g.key_color.rgb * shadow_at(i.shadow)
    + light(N, V, g.fill_dir.xyz, diff, F0, r) * g.fill_color.rgb;
  let ao = mix(1.0, 0.35 + 0.65 * smoothstep(0.0, 0.5, i.world.y), clamp(-N.y, 0.0, 1.0));
  let irr = mix(table(i.world.xz, 9.0) * 0.8, g.sky.rgb, N.y * 0.5 + 0.5);
  let ab = env_brdf(r, NV);
  col += (diff * irr + env(i.world, reflect(-V, N), r) * (F0 * ab.x + ab.y)) * ao;
  col += m.highlight.rgb * m.highlight.a * (0.25 + (1.0 - NV) * (1.0 - NV));
  var c = pow(aces(col), vec3<f32>(1.0 / 2.2));
  if (g.flags.x > 0.5) {
    c = pow(c, vec3<f32>(2.2));
  }
  return vec4<f32>(c, 1.0);
}

@vertex
fn vs_shadow(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {
  return g.light_vp * (m.model * vec4<f32>(p, 1.0));
}

// Shadow receiver: outputs only darkening alpha over the CSS table.
struct GroundOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) world: vec3<f32>,
  @location(1) shadow: vec3<f32>,
  @location(2) quad: vec2<f32>,
};

@vertex
fn vs_ground(@builtin(vertex_index) vi: u32) -> GroundOut {
  let q = vec2<f32>(f32(vi & 1u) * 2.0 - 1.0, f32(vi >> 1u) * 2.0 - 1.0);
  let w = vec3<f32>(q.x * cam.ground.x, 0.0, q.y * cam.ground.y);
  var o: GroundOut;
  o.pos = cam.view_proj * vec4<f32>(w, 1.0);
  o.world = w;
  o.shadow = (g.shadow_mat * vec4<f32>(w, 1.0)).xyz;
  o.quad = q;
  return o;
}

fn pcf(s: vec3<f32>, frag: vec2<f32>) -> f32 {
  if (any(s.xy < vec2<f32>(0.0)) || any(s > vec3<f32>(1.0))) {
    return 1.0;
  }
  var disc = array<vec2<f32>, 12>(
    vec2<f32>(-0.326, -0.406), vec2<f32>(-0.840, -0.074), vec2<f32>(-0.696, 0.457), vec2<f32>(-0.203, 0.621),
    vec2<f32>(0.962, -0.195), vec2<f32>(0.473, -0.480), vec2<f32>(0.519, 0.767), vec2<f32>(0.185, -0.893),
    vec2<f32>(0.507, 0.064), vec2<f32>(0.896, 0.412), vec2<f32>(-0.322, -0.933), vec2<f32>(-0.792, -0.598));
  let texel = 5.0 / vec2<f32>(textureDimensions(shadow_tex));
  let a = fract(52.9829189 * fract(dot(frag, vec2<f32>(0.06711056, 0.00583715)))) * 6.2831853;
  let rot = mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a));
  var sum = 0.0;
  for (var i = 0; i < 12; i++) {
    sum += textureSampleCompareLevel(shadow_tex, shadow_smp, s.xy + rot * disc[i] * texel, s.z - 0.002);
  }
  return sum / 12.0;
}

// Quilez: soft shadow of a sphere.
fn sphere_shadow(ro: vec3<f32>, rd: vec3<f32>, s: vec4<f32>) -> f32 {
  let oc = ro - s.xyz;
  let b = dot(oc, rd);
  let h = b * b - dot(oc, oc) + s.w * s.w;
  let d = sqrt(max(0.0, s.w * s.w - h)) - s.w;
  let t = -b - sqrt(max(h, 0.0));
  if (t < 0.0) {
    return 1.0;
  }
  return smoothstep(0.0, 1.0, 5.0 * d / t);
}

@fragment
fn fs_ground(i: GroundOut) -> @location(0) vec4<f32> {
  // Only fragments near a die's shadow foot need the 12 shadow map taps.
  var vis = 1.0;
  var ao = 1.0;
  var near = false;
  let sphere = cam.ground.z > 0.5;
  let n = i32(cam.ground.w);
  for (var k = 0; k < 12; k++) {
    if (k >= n) {
      break;
    }
    let o = cam.occ[k];
    let d = o.xyz - i.world;
    let inv = inverseSqrt(dot(d, d));
    let foot = o.xz - g.key_dir.xz * (o.y / g.key_dir.y) - i.world.xz;
    let radius = 2.7 * o.w + 0.15;
    if (sphere) {
      vis = min(vis, sphere_shadow(i.world, g.key_dir.xyz, o));
    } else if (dot(foot, foot) < radius * radius) {
      near = true;
    }
    ao *= 1.0 - clamp(o.w * o.w * max(d.y, 0.0) * inv * inv * inv, 0.0, 1.0);
  }
  if (near) {
    vis = pcf(i.shadow, i.pos.xy);
  }
  var a = clamp((1.0 - vis) * cam.strength.x + (1.0 - ao) * cam.strength.y, 0.0, 0.85);
  if (sphere) {
    a *= 1.0 - smoothstep(0.5, 1.0, length(i.quad)); // previews: no quad edge
  }
  return vec4<f32>(0.0, 0.0, 0.0, a);
}
