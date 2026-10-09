// renderer3d.js — WebGL2 renderer for the physical dice: lit, textured meshes from dice3d.js on a
// transparent fullscreen canvas. The table itself stays the CSS background; the renderer draws the
// dice, their soft shadows (one cached shadow map plus analytic contact occlusion) and reflects the
// real table texture in the dice. Every camera (table, tray previews) renders into the whole canvas
// through a clip-space remap of its logical rect, so dice and shadows cross table/tray borders.
// Render on demand only: no RAF of its own; async assets call `onChange`.
import { ATLAS_COLS } from './dice3d.js';
import { MAX_DICE } from './dice.js';

const DPR_MAX = 2;
const FOV = 32 * Math.PI / 180, TAN = Math.tan(FOV / 2);
const PREVIEW_FOV = 28 * Math.PI / 180, PREVIEW_TAN = Math.tan(PREVIEW_FOV / 2), PREVIEW_TILT = 0.3;
const SHADOW_SIZE = 1024;
const MAX_OCC = 12;
const ATLAS_CACHE = MAX_DICE + 4; // ≥ dice drawn per frame, so per-throw face mappings never thrash
const GUTTER = 6; // CSS px a tray's scroll clip is grown by, so edges aren't chopped flush
const FONT = '800 100px system-ui, -apple-system, "Segoe UI", sans-serif';
const PIPS = [[], [4], [0, 8], [0, 4, 8], [0, 2, 6, 8], [0, 2, 4, 6, 8], [0, 2, 3, 5, 6, 8]];

const norm = v => { const l = Math.hypot(...v); return [v[0] / l, v[1] / l, v[2] / l]; };
const KEY_DIR = norm([-0.42, 1, -0.62]); // from the upper left, like the CSS drop shadows
const FILL_DIR = norm([0.7, 0.45, 0.6]);

// Same colors as style.css: [die, pip] for the palette; skins add [roughness, metalness].
const PLAIN = {
  white: ['#f7f5ef', '#171717'], blue: ['#1652f0', '#fff'], yellow: ['#ffcc00', '#1f2430'], green: ['#00b347', '#fff'],
  orange: ['#ff7300', '#fff'], purple: ['#7a1fff', '#fff'], red: ['#e3101e', '#fff'], brown: ['#8a5a32', '#fff'],
  turquoise: ['#00c2b8', '#002826'], pink: ['#ff2d95', '#fff'], gray: ['#343a46', '#fff'],
};
const SKINS = {
  opal: ['#c0cceb', '#141414', 0.18], wood: ['#dd7d36', '#141414', 0.55], galaxy: ['#321474', '#e2ac3c', 0.2],
  jade: ['#267947', '#e2ac3c', 0.22], amber: ['#e67113', '#141414', 0.15], terrazzo: ['#edd7c4', '#141414', 0.38],
  ruby: ['#aa0d0b', '#f2f2f2', 0.12], steel: ['#9c9393', '#141414', 0.3, 0.85], prism: ['#c9b8d3', '#b8860b', 0.14],
  marble: ['#32271b', '#e2ac3c', 0.2], ice: ['#3a99df', '#141414', 0.1], walnut: ['#b4511e', '#1e1410', 0.5],
};
// Average colors of the table textures (style.css), used until the image has loaded.
const SURFACES = { casino: '#09491f', oak: '#8f8071', crafts: '#e5d8c4', bluewood: '#3a4a57', terracotta: '#cf6736' };

function material(color) {
  const skin = color?.startsWith('skin-') && SKINS[color.slice(5)];
  if (skin) return { die: skin[0], pip: skin[1], rough: skin[2], metal: skin[3] ?? 0, skin: color.slice(5) };
  const [die, pip] = PLAIN[color] ?? ['#f7f5ef', '#191919'];
  return { die, pip, rough: 0.3, metal: 0 };
}

const hex = s => {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map(c => c + c).join('') : m[1];
  return [0, 2, 4].map(i => parseInt(h.slice(i, i + 2), 16));
};
const linear = rgb => rgb.map(c => (c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

// ---- matrices (column-major Float32Array(16)) ----
function perspective(out, fovy, aspect, near, far) {
  const f = 1 / Math.tan(fovy / 2), nf = 1 / (near - far);
  out.fill(0);
  out[0] = f / aspect; out[5] = f; out[10] = (far + near) * nf; out[11] = -1; out[14] = 2 * far * near * nf;
  return out;
}
function ortho(out, l, r, b, t, n, f) {
  out.fill(0);
  out[0] = 2 / (r - l); out[5] = 2 / (t - b); out[10] = -2 / (f - n);
  out[12] = -(r + l) / (r - l); out[13] = -(t + b) / (t - b); out[14] = -(f + n) / (f - n); out[15] = 1;
  return out;
}
function lookAt(out, eye, target, up) {
  const f = norm([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
  const s = norm([f[1] * up[2] - f[2] * up[1], f[2] * up[0] - f[0] * up[2], f[0] * up[1] - f[1] * up[0]]);
  const u = [s[1] * f[2] - s[2] * f[1], s[2] * f[0] - s[0] * f[2], s[0] * f[1] - s[1] * f[0]];
  out.set([s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0,
    -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]), -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
    f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2], 1]);
  return out;
}
function mul(out, a, b) {
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
    out[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
  }
  return out;
}
// Rotation of quaternion q (same layout as physics3d mat3) plus translation p.
function model(out, q, p) {
  const [x, y, z, w] = q;
  out[0] = 1 - 2 * (y * y + z * z); out[1] = 2 * (x * y + z * w); out[2] = 2 * (x * z - y * w); out[3] = 0;
  out[4] = 2 * (x * y - z * w); out[5] = 1 - 2 * (x * x + z * z); out[6] = 2 * (y * z + x * w); out[7] = 0;
  out[8] = 2 * (x * z + y * w); out[9] = 2 * (y * z - x * w); out[10] = 1 - 2 * (x * x + y * y); out[11] = 0;
  out[12] = p[0]; out[13] = p[1]; out[14] = p[2]; out[15] = 1;
  return out;
}
const SHADOW_BIAS = new Float32Array([0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0.5, 0.5, 0.5, 1]);

// ---- shaders ----
const DIE_VS = `#version 300 es
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec2 aUV;
uniform mat4 uModel, uViewProj, uShadowMat;
out vec3 vWorld, vNormal, vShadow;
out vec2 vUV;
void main() {
  vec4 w = uModel * vec4(aPos, 1.);
  vWorld = w.xyz;
  vNormal = mat3(uModel) * aNormal;
  vUV = aUV;
  vShadow = (uShadowMat * vec4(w.xyz + vNormal * .02, 1.)).xyz;
  gl_Position = uViewProj * w;
}`;

const DIE_FS = `#version 300 es
precision highp float;
precision highp sampler2DShadow;
in vec3 vWorld, vNormal, vShadow;
in vec2 vUV;
uniform sampler2D uAtlas, uEnv;
uniform sampler2DShadow uShadow;
uniform int uShadowOn;
uniform vec4 uEnvMap; // table texture uv = xz * x + yz; w = texture size
uniform vec3 uCam, uKeyDir, uKeyColor, uFillDir, uFillColor, uSky;
uniform vec2 uMat; // roughness, metalness
uniform vec4 uHighlight;
out vec4 outColor;
const float PI = 3.14159265;

float shadow() {
  vec3 s = vShadow;
  if (uShadowOn == 0 || any(lessThan(s.xy, vec2(0))) || any(greaterThan(s, vec3(1)))) return 1.;
  vec2 t = 1.5 / vec2(textureSize(uShadow, 0));
  float sum = 0.;
  for (int i = -1; i <= 1; i++) for (int j = -1; j <= 1; j++) sum += texture(uShadow, vec3(s.xy + vec2(i, j) * t, s.z - .0015));
  return sum / 9.;
}
vec3 light(vec3 N, vec3 V, vec3 L, vec3 diff, vec3 F0, float rough) {
  float NL = dot(N, L);
  if (NL <= 0.) return vec3(0);
  vec3 H = normalize(V + L);
  float NH = max(dot(N, H), 0.), NV = max(dot(N, V), 1e-4), VH = max(dot(V, H), 0.);
  float r = max(rough, .18); // the key light is a softbox, not a point
  float a2 = r * r * r * r;
  float d = NH * NH * (a2 - 1.) + 1.;
  float k = (r + 1.) * (r + 1.) / 8.;
  vec3 F = F0 + (1. - F0) * pow(1. - VH, 5.);
  vec3 spec = a2 / (PI * d * d) * F / (4. * (NL * (1. - k) + k) * (NV * (1. - k) + k));
  return (diff / PI * (1. - F) + spec) * NL;
}
vec2 envBRDF(float r, float NV) { // Karis' analytic split-sum approximation
  vec4 r4 = r * vec4(-1, -.0275, -.572, .022) + vec4(1, .0425, 1.04, -.04);
  float a = min(r4.x * r4.x, exp2(-9.28 * NV)) * r4.x + r4.y;
  return vec2(-1.04, 1.04) * a + r4.zw;
}
vec3 table(vec2 xz, float lod) { return textureLod(uEnv, xz * uEnvMap.x + uEnvMap.yz, lod).rgb; }
vec3 env(vec3 P, vec3 R, float r) {
  vec3 sky = uSky * (.55 + .7 * max(R.y, 0.)) + uKeyColor * .06 * pow(max(dot(R, uKeyDir), 0.), mix(64., 4., r));
  float t = max(P.y, 0.) / max(-R.y, 1e-3);
  float lod = log2(max((.05 + r * 1.2) * t * uEnvMap.x * uEnvMap.w, 1.));
  return mix(table(P.xz + R.xz * t, lod) * .9, sky, smoothstep(-.12, .08, R.y));
}
vec3 aces(vec3 x) { return clamp(x * (2.51 * x + .03) / (x * (2.43 * x + .59) + .14), 0., 1.); }
void main() {
  vec3 N = normalize(vNormal), V = normalize(uCam - vWorld);
  vec3 albedo = texture(uAtlas, vUV).rgb;
  float r = uMat.x, metal = uMat.y, NV = max(dot(N, V), 1e-4);
  vec3 F0 = mix(vec3(.04), albedo, metal), diff = albedo * (1. - metal);
  vec3 col = light(N, V, uKeyDir, diff, F0, r) * uKeyColor * shadow() + light(N, V, uFillDir, diff, F0, r) * uFillColor;
  float ao = mix(1., .35 + .65 * smoothstep(0., .5, vWorld.y), clamp(-N.y, 0., 1.));
  vec3 irr = mix(table(vWorld.xz, 9.) * .8, uSky, N.y * .5 + .5);
  vec2 ab = envBRDF(r, NV);
  col += (diff * irr + env(vWorld, reflect(-V, N), r) * (F0 * ab.x + ab.y)) * ao;
  col += uHighlight.rgb * uHighlight.a * (.25 + (1. - NV) * (1. - NV));
  outColor = vec4(pow(aces(col), vec3(1. / 2.2)), 1.);
}`;

const SHADOW_VS = `#version 300 es
layout(location=0) in vec3 aPos;
uniform mat4 uModel, uLightVP;
void main() { gl_Position = uLightVP * uModel * vec4(aPos, 1.); }`;
const SHADOW_FS = `#version 300 es
void main() {}`;

// Shadow receiver: outputs only darkening alpha over the CSS table.
const GROUND_VS = `#version 300 es
layout(location=0) in vec2 aQuad;
uniform vec2 uExtent;
uniform mat4 uViewProj, uShadowMat;
out vec3 vWorld, vShadow;
out vec2 vQuad;
void main() {
  vQuad = aQuad;
  vWorld = vec3(aQuad.x * uExtent.x, 0., aQuad.y * uExtent.y);
  vShadow = (uShadowMat * vec4(vWorld, 1.)).xyz;
  gl_Position = uViewProj * vec4(vWorld, 1.);
}`;
const GROUND_FS = `#version 300 es
precision highp float;
precision highp sampler2DShadow;
in vec3 vWorld, vShadow;
in vec2 vQuad;
uniform sampler2DShadow uShadow;
uniform int uMode; // 0: shadow map (table), 1: analytic sphere shadow (previews)
uniform vec4 uOcc[${MAX_OCC}];
uniform int uOccN;
uniform vec3 uKeyDir;
uniform vec2 uStrength;
out vec4 outColor;
const vec2 DISC[12] = vec2[](vec2(-.326, -.406), vec2(-.840, -.074), vec2(-.696, .457), vec2(-.203, .621),
  vec2(.962, -.195), vec2(.473, -.480), vec2(.519, .767), vec2(.185, -.893), vec2(.507, .064), vec2(.896, .412),
  vec2(-.322, -.933), vec2(-.792, -.598));
float pcf(vec3 s) {
  if (any(lessThan(s.xy, vec2(0))) || any(greaterThan(s, vec3(1)))) return 1.;
  vec2 texel = 5. / vec2(textureSize(uShadow, 0));
  float a = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(.06711056, .00583715)))) * 6.2831853;
  mat2 rot = mat2(cos(a), sin(a), -sin(a), cos(a));
  float sum = 0.;
  for (int i = 0; i < 12; i++) sum += texture(uShadow, vec3(s.xy + rot * DISC[i] * texel, s.z - .002));
  return sum / 12.;
}
float sphereShadow(vec3 ro, vec3 rd, vec4 s) { // Quilez: soft shadow of a sphere
  vec3 oc = ro - s.xyz;
  float b = dot(oc, rd), h = b * b - dot(oc, oc) + s.w * s.w;
  float d = sqrt(max(0., s.w * s.w - h)) - s.w, t = -b - sqrt(max(h, 0.));
  return t < 0. ? 1. : smoothstep(0., 1., 5. * d / t);
}
void main() {
  float vis = uMode == 0 ? pcf(vShadow) : 1., ao = 1.;
  for (int i = 0; i < ${MAX_OCC}; i++) {
    if (i >= uOccN) break;
    vec4 o = uOcc[i];
    vec3 d = o.xyz - vWorld;
    float l = length(d);
    if (uMode == 1) vis = min(vis, sphereShadow(vWorld, uKeyDir, o));
    ao *= 1. - clamp(o.w * o.w / (l * l) * max(d.y / l, 0.), 0., 1.);
  }
  float a = clamp((1. - vis) * uStrength.x + (1. - ao) * uStrength.y, 0., .85);
  if (uMode == 1) a *= 1. - smoothstep(.5, 1., length(vQuad)); // previews: no quad edge
  outColor = vec4(0, 0, 0, a);
}`;

function program(gl, vs, fs) {
  const p = gl.createProgram();
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS) && !gl.isContextLost()) throw new Error(`Dice shader failed to compile: ${gl.getShaderInfoLog(s)}`);
    gl.attachShader(p, s);
    gl.deleteShader(s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS) && !gl.isContextLost()) throw new Error(`Dice shader failed to link: ${gl.getProgramInfoLog(p)}`);
  const u = {};
  const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS) ?? 0;
  for (let i = 0; i < n; i++) {
    const name = gl.getActiveUniform(p, i).name.replace(/\[0\]$/, '');
    u[name] = gl.getUniformLocation(p, name);
  }
  return { p, u };
}

// ---- texture atlas (2D canvas): faces with numbers, pips and d4 corner labels ----
let capHeight = 0; // cap height of the digits in FONT, in px
const AMBIGUOUS = { 0: '0', 1: '1', 6: '9', 8: '8', 9: '6' };
// 6/9-style values that read as another value upside down get an underline, like real dice.
function ambiguous(value, sides) {
  const s = String(value);
  if (![...s].every(c => c in AMBIGUOUS)) return false;
  const turned = [...s].reverse().map(c => AMBIGUOUS[c]).join('');
  return turned !== s && turned[0] !== '0' && +turned >= 1 && +turned <= sides;
}

// Draws `text` centered at the origin of the current transform (y down), fitted into a free circle
// of `radius` px and no taller than real dice digits.
function glyphs(ctx, text, radius, ink, underline) {
  ctx.font = FONT;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'alphabetic';
  capHeight ||= ctx.measureText('0').actualBoundingBoxAscent || 72;
  const width = ctx.measureText(text).width;
  const s = Math.min(0.85 * radius / Math.hypot(width / 2, capHeight * 0.62), 1.1 * radius / capHeight);
  ctx.scale(s, s);
  const y = capHeight / 2;
  ctx.fillStyle = 'rgba(0,0,0,.28)'; // engraving: dark rim on the upper left
  ctx.fillText(text, -2.5, y - 2.5);
  ctx.fillStyle = ink;
  ctx.fillText(text, 0, y);
  if (underline) ctx.fillRect(-width * 0.3, y + capHeight * 0.14, width * 0.6, capHeight * 0.1);
}

// The center of the skin's blank face (the baked rounded rim stays outside).
function drawCrop(ctx, img, x, y, size) {
  const s = Math.min(img.naturalWidth, img.naturalHeight) * 0.72;
  ctx.drawImage(img, (img.naturalWidth - s) / 2, (img.naturalHeight - s) / 2, s, s, x, y, size, size);
}

function drawPips(ctx, value, x0, y0, C, mat, pipsImg) {
  for (const g of PIPS[value] ?? []) {
    const x = x0 + (0.25 + 0.25 * (g % 3)) * C, y = y0 + (0.25 + 0.25 * Math.floor(g / 3)) * C;
    ctx.save();
    ctx.beginPath();
    if (pipsImg) {
      // Copy the painted pip at the same grid spot of pips.webp (24/50/76 %, see style.css).
      const r = 0.118 * C, iw = pipsImg.naturalWidth, ih = pipsImg.naturalHeight, sr = 0.115 * iw;
      const sx = iw * (0.24 + 0.26 * (g % 3)), sy = ih * (0.24 + 0.26 * Math.floor(g / 3));
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.clip();
      ctx.drawImage(pipsImg, sx - sr, sy - sr, sr * 2, sr * 2, x - r, y - r, r * 2, r * 2);
    } else {
      const r = 0.095 * C;
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = mat.pip;
      ctx.fill();
      const shade = ctx.createLinearGradient(x - r, y - r, x + r, y + r); // drilled: shaded upper wall
      shade.addColorStop(0, 'rgba(0,0,0,.4)');
      shade.addColorStop(0.55, 'rgba(0,0,0,0)');
      shade.addColorStop(1, 'rgba(255,255,255,.14)');
      ctx.fillStyle = shade;
      ctx.fill();
    }
    ctx.restore();
  }
}

// One square cell per face (uniform scale, flat y up) plus the edge cell = die color; uploaded with
// FLIP_Y so the dice3d uv v = 1 − (row + 1 − fy) / rows lands on canvas y = (row + 1 − fy) · cell.
// `values[k − 1]` is the number printed for geometric face value k (undefined: identity).
function buildAtlas(shape, mat, numbered, faceImg, pipsImg, values) {
  const shown = v => values?.[v - 1] ?? v;
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  const C = shape.faces.length > 24 ? 128 : 256;
  const rows = shape.rows ?? Math.ceil((shape.faces.length + 1) / ATLAS_COLS);
  canvas.width = ATLAS_COLS * C;
  canvas.height = rows * C;
  ctx.fillStyle = mat.die;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  shape.faces.forEach((face, i) => {
    const cell = face.cell ?? i, x0 = (cell % ATLAS_COLS) * C, y0 = Math.floor(cell / ATLAS_COLS) * C;
    ctx.save();
    ctx.beginPath();
    ctx.rect(x0, y0, C, C);
    ctx.clip();
    if (faceImg) drawCrop(ctx, faceImg, x0, y0, C);
    if (shape.sides === 6 && !numbered) drawPips(ctx, shown(face.value), x0, y0, C, mat, pipsImg);
    // d4 labels: the value of the face opposite each corner, upright toward it (dice3d computes them).
    else for (const label of face.labels) {
      const value = shown(label.value);
      ctx.setTransform(1, 0, 0, 1, x0 + label.x * C, y0 + (1 - label.y) * C);
      ctx.rotate(Math.atan2(label.up[0], label.up[1]));
      glyphs(ctx, String(value), label.size * C, mat.pip, ambiguous(value, shape.sides));
    }
    ctx.restore();
  });
  return canvas;
}

export class DiceRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.onChange = null; // called when an async asset is ready or the context came back
    this.onContextLost = null;
    const gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: true, depth: true, stencil: false, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 is not available');
    this.gl = gl;
    this.images = new Map(); // url → { img, ready }
    this.surface = undefined;
    this.theme = undefined;
    this.accent = linear([255, 176, 46]);
    this.tableColor = '#12141a';
    this.view = { left: 0, top: 0, dpr: 1 };
    this.cam = {};
    this.mat = { model: new Float32Array(16), view: new Float32Array(16), proj: new Float32Array(16), vp: new Float32Array(16), light: new Float32Array(16), shadow: new Float32Array(16) };
    this.occ = new Float32Array(MAX_OCC * 4);
    this.envMap = new Float32Array(4);
    this.lost = event => { event.preventDefault(); this.onContextLost?.(); };
    this.restored = () => { this.init(); this.onChange?.(); };
    canvas.addEventListener('webglcontextlost', this.lost);
    canvas.addEventListener('webglcontextrestored', this.restored);
    this.media = matchMedia('(prefers-color-scheme: light)');
    this.themeChanged = () => {
      if (this.theme === 'auto') { this.setSurface(this.surface, this.theme); this.onChange?.(); }
    };
    this.media.addEventListener('change', this.themeChanged);
    this.init();
  }

  // Creates every GPU resource; again after a context restore (CPU images survive).
  init() {
    const gl = this.gl;
    this.meshes = new Map();
    this.atlases = new Map();
    this.textures = new Map(); // table textures and solid colors
    this.shadowKey = NaN;
    this.lightKey = '';
    this.die = program(gl, DIE_VS, DIE_FS);
    this.depth = program(gl, SHADOW_VS, SHADOW_FS);
    this.ground = program(gl, GROUND_VS, GROUND_FS);
    const sky = linear([140, 150, 168]), key = [2.8, 2.66, 2.46], fill = linear([140, 158, 190]).map(c => c * 0.45);
    gl.useProgram(this.die.p);
    const u = this.die.u;
    gl.uniform1i(u.uAtlas, 0); gl.uniform1i(u.uEnv, 1); gl.uniform1i(u.uShadow, 2);
    gl.uniform3fv(u.uKeyDir, KEY_DIR); gl.uniform3fv(u.uKeyColor, key);
    gl.uniform3fv(u.uFillDir, FILL_DIR); gl.uniform3fv(u.uFillColor, fill);
    gl.uniform3fv(u.uSky, sky);
    gl.useProgram(this.ground.p);
    gl.uniform1i(this.ground.u.uShadow, 2);
    gl.uniform3fv(this.ground.u.uKeyDir, KEY_DIR);

    this.quad = gl.createVertexArray();
    gl.bindVertexArray(this.quad);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);

    this.shadowTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.shadowTex);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.DEPTH_COMPONENT24, SHADOW_SIZE, SHADOW_SIZE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_FUNC, gl.LEQUAL);
    this.shadowFbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.shadowFbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, this.shadowTex, 0);
    gl.drawBuffers([gl.NONE]);
    gl.readBuffer(gl.NONE);
    const status = gl.checkFramebufferStatus(gl.FRAMEBUFFER);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (status !== gl.FRAMEBUFFER_COMPLETE && !gl.isContextLost()) throw new Error(`Dice shadow framebuffer incomplete (0x${status.toString(16)})`);
    this.aniso = gl.getExtension('EXT_texture_filter_anisotropic');
    // getParameter stalls the GPU pipeline; query once, not per texture upload.
    if (this.aniso) this.maxAniso = Math.min(8, gl.getParameter(this.aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT));
    gl.enable(gl.DEPTH_TEST);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  }

  // surface: '' or a backgrounds/ name; theme: 'auto' | 'light' | 'dark' (the plain --bg color).
  setSurface(surface, theme) {
    const light = theme === 'light' || theme !== 'dark' && this.media.matches;
    if (surface === this.surface && theme === this.theme && light === this.light) return;
    this.surface = surface;
    this.theme = theme;
    this.light = light;
    this.tableColor = SURFACES[surface] ?? (light ? '#eef0f4' : '#12141a');
    const accent = hex(getComputedStyle(document.documentElement).getPropertyValue('--accent'));
    if (accent) this.accent = linear(accent);
  }

  // ---- assets ----
  image(path) {
    let entry = this.images.get(path);
    if (!entry) {
      entry = { img: new Image(), ready: false };
      this.images.set(path, entry);
      entry.img.decoding = 'async';
      entry.img.onload = () => {
        entry.ready = true;
        for (const [key, atlas] of this.atlases) if (atlas.pending) { this.gl.deleteTexture(atlas.tex); this.atlases.delete(key); }
        this.onChange?.();
      };
      entry.img.src = new URL(path, import.meta.url).href;
    }
    return entry.ready ? entry.img : null;
  }

  upload(source, flip) {
    const gl = this.gl, tex = gl.createTexture();
    gl.activeTexture(gl.TEXTURE0); // never disturb the env/shadow units bound for the frame
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, flip);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    if (source instanceof Uint8Array) gl.texImage2D(gl.TEXTURE_2D, 0, gl.SRGB8_ALPHA8, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, source);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.SRGB8_ALPHA8, gl.RGBA, gl.UNSIGNED_BYTE, source);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    if (this.aniso) gl.texParameterf(gl.TEXTURE_2D, this.aniso.TEXTURE_MAX_ANISOTROPY_EXT, this.maxAniso);
    return tex;
  }

  // The table as the dice see it: the real background texture, or a solid color.
  env() {
    const surface = this.surface, color = this.tableColor;
    const img = SURFACES[surface] ? this.image(`backgrounds/${surface}.webp`) : null;
    const key = img ? surface : color;
    let entry = this.textures.get(key);
    if (!entry) {
      entry = img ? { tex: this.upload(img, false), size: img.naturalWidth } : { tex: this.upload(new Uint8Array([...hex(color), 255]), false), size: 1 };
      this.textures.set(key, entry);
    }
    return entry;
  }

  mesh(shape) {
    let m = this.meshes.get(shape.sides); // geometry is a pure function of the side count
    if (m) return m;
    const gl = this.gl, vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    [[shape.positions, 3], [shape.normals, 3], [shape.uvs, 2]].forEach(([data, size], loc) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
    });
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, shape.indices, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    // Hull planes for exact picking: collision core grown by the bevel margin.
    const v = shape.vertices, margin = shape.margin ?? 0;
    const planes = shape.faces.map(f => {
      let d = -Infinity;
      for (let i = 0; i < v.length; i += 3) d = Math.max(d, f.normal[0] * v[i] + f.normal[1] * v[i + 1] + f.normal[2] * v[i + 2]);
      return [...f.normal, d + margin];
    });
    m = { vao, count: shape.indices.length, planes, occ: (shape.inradius + shape.radius) / 2 };
    this.meshes.set(shape.sides, m);
    return m;
  }

  atlas(shape, color, numbered, values, valuesKey) {
    numbered = shape.sides === 6 && !!numbered;
    const key = `${shape.sides}|${color ?? ''}|${numbered}|${values ? valuesKey ?? values.join() : ''}`;
    let a = this.atlases.get(key);
    if (a) {
      this.atlases.delete(key); // least recently used goes first
      this.atlases.set(key, a);
      return a;
    }
    const mat = material(color);
    const usesPips = shape.sides === 6 && !numbered;
    const faceImg = mat.skin ? this.image(`skins/${mat.skin}/face.webp`) : null;
    const pipsImg = mat.skin && usesPips ? this.image(`skins/${mat.skin}/pips.webp`) : null;
    a = {
      tex: this.upload(buildAtlas(shape, mat, numbered, faceImg, pipsImg, values), true),
      rough: mat.rough, metal: mat.metal,
      pending: !!mat.skin && (!faceImg || usesPips && !pipsImg),
    };
    this.atlases.set(key, a);
    if (this.atlases.size > ATLAS_CACHE) {
      const [oldKey, old] = this.atlases.entries().next().value;
      this.gl.deleteTexture(old.tex);
      this.atlases.delete(oldKey);
    }
    return a;
  }

  // ---- cameras ----
  // Table: straight down with perspective, framing the bounds at y = 0 exactly onto tableRect.
  camera(rect, bounds) {
    const c = this.cam;
    if (c.w !== rect.width || c.h !== rect.height || c.bx !== bounds.x || c.bz !== bounds.z) {
      const aspect = rect.width / rect.height, half = Math.max(bounds.z, bounds.x / aspect), H = half / TAN;
      Object.assign(c, { w: rect.width, h: rect.height, bx: bounds.x, bz: bounds.z, aspect, half, H });
      c.vp = mul(new Float32Array(16), perspective(this.mat.proj, FOV, aspect, Math.max(0.1, H - 8), H + 1), lookAt(this.mat.view, [0, H, 0], [0, 0, 0], [0, 0, -1]));
    }
    return c;
  }

  // Client-pixel center and diameter of a table body's bounding sphere.
  project(body, rect, bounds) {
    const { H } = this.camera(rect, bounds);
    const k = rect.height / (2 * TAN * Math.max(0.1, H - body.pos[1]));
    return { x: rect.left + rect.width / 2 + body.pos[0] * k, y: rect.top + rect.height / 2 + body.pos[2] * k, size: 2 * body.shape.radius * k };
  }

  // World point at `height` under the client pixel.
  unproject(clientX, clientY, rect, bounds, height = 1.6) {
    const { H } = this.camera(rect, bounds);
    const k = rect.height / (2 * TAN * H), f = (H - height) / H;
    return [(clientX - rect.left - rect.width / 2) / k * f, height, (clientY - rect.top - rect.height / 2) / k * f];
  }

  // Nearest body whose hull the view ray through the client pixel hits, or null.
  raycast(clientX, clientY, bodies, rect, bounds) {
    const { H } = this.camera(rect, bounds);
    const g = this.unproject(clientX, clientY, rect, bounds, 0);
    const dir = norm([g[0], -H, g[2]]);
    let best = null, bestT = Infinity;
    for (const body of bodies) {
      if (body.dead) continue;
      const m = this.mesh(body.shape);
      // Ray into the body frame: inverse rotation = rotation by the conjugate quaternion.
      const [qx, qy, qz, qw] = body.quat;
      const inv = v => {
        const tx = 2 * (-qy * v[2] + qz * v[1]), ty = 2 * (-qz * v[0] + qx * v[2]), tz = 2 * (-qx * v[1] + qy * v[0]);
        return [v[0] + qw * tx + (-qy * tz + qz * ty), v[1] + qw * ty + (-qz * tx + qx * tz), v[2] + qw * tz + (-qx * ty + qy * tx)];
      };
      const o = inv([-body.pos[0], H - body.pos[1], -body.pos[2]]), d = inv(dir);
      let t0 = 0, t1 = Infinity;
      for (const [nx, ny, nz, dist] of m.planes) {
        const denom = nx * d[0] + ny * d[1] + nz * d[2], num = dist - (nx * o[0] + ny * o[1] + nz * o[2]);
        if (Math.abs(denom) < 1e-9) { if (num < 0) { t0 = Infinity; break; } continue; }
        const t = num / denom;
        if (denom < 0) t0 = Math.max(t0, t); else t1 = Math.min(t1, t);
      }
      if (t0 > t1) continue;
      if (t0 < bestT) { bestT = t0; best = body; }
    }
    return best;
  }

  // Scissor to the scroll clip (grown by GUTTER) ∩ canvas, or none without a clip. False if empty.
  scissor(clip) {
    const { gl, view: { dpr, left, top } } = this, W = this.canvas.width, H = this.canvas.height;
    if (!clip) { gl.disable(gl.SCISSOR_TEST); return true; }
    const x0 = Math.max(0, Math.floor((clip.left - GUTTER - left) * dpr));
    const x1 = Math.min(W, Math.ceil((clip.left + clip.width + GUTTER - left) * dpr));
    const y0 = Math.max(0, H - Math.ceil((clip.top + clip.height + GUTTER - top) * dpr));
    const y1 = Math.min(H, H - Math.floor((clip.top - GUTTER - top) * dpr));
    if (x1 <= x0 || y1 <= y0) return false;
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(x0, y0, x1 - x0, y1 - y0);
    return true;
  }

  // Left-multiplies view-projection `m` so the camera framed onto client `rect` renders into the
  // full-canvas viewport: clip x → sx·x + tx·w, y → sy·y + ty·w. project/unproject keep using `rect`.
  frame(m, rect) {
    const { left, top, width, height } = this.view;
    const sx = rect.width / width, tx = (2 * (rect.left - left) + rect.width) / width - 1;
    const sy = rect.height / height, ty = 1 - (2 * (rect.top - top) + rect.height) / height;
    for (let c = 0; c < 16; c += 4) {
      m[c] = sx * m[c] + tx * m[c + 3];
      m[c + 1] = sy * m[c + 1] + ty * m[c + 3];
    }
    return m;
  }

  // Tray preview camera into mat.vp, framed onto `rect`: the die rests on its lowest point (lifted
  // while held), seen slightly from the front.
  previewCamera(body, rect) {
    const r = body.shape.radius, v = body.shape.vertices, q = model(this.mat.model, body.quat, [0, 0, 0]);
    let low = Infinity;
    for (let i = 0; i < v.length; i += 3) low = Math.min(low, q[1] * v[i] + q[5] * v[i + 1] + q[9] * v[i + 2]);
    low -= body.shape.margin ?? 0;
    const y = -low + (body.held ? r * 0.6 : 0), dist = r * 1.1 / Math.sin(PREVIEW_FOV / 2);
    const eye = [0, y + dist * Math.cos(PREVIEW_TILT), dist * Math.sin(PREVIEW_TILT)];
    mul(this.mat.vp, perspective(this.mat.proj, PREVIEW_FOV, rect.width / rect.height, dist * 0.5, dist * 2 + r * 4), lookAt(this.mat.view, eye, [0, y, 0], [0, 0, -1]));
    this.frame(this.mat.vp, rect);
    return { eye, y, dist };
  }

  // Table texture coordinates for world xz, given where the world origin sits (client px) and the
  // scale (px per unit): CSS draws it with `center / cover` over the viewport.
  setEnvMap(cx, cy, k, size) {
    const { width, height } = this.view, S = Math.max(width, height);
    this.envMap[0] = k / S;
    this.envMap[1] = (cx - this.view.left - (width - S) / 2) / S;
    this.envMap[2] = (cy - this.view.top - (height - S) / 2) / S;
    this.envMap[3] = size;
  }

  drawMesh(m) {
    const gl = this.gl;
    gl.bindVertexArray(m.vao);
    gl.drawElements(gl.TRIANGLES, m.count, gl.UNSIGNED_SHORT, 0);
  }

  drawDie(body, pos) {
    const { gl } = this, u = this.die.u;
    const a = this.atlas(body.shape, body.color, body.numbered, body.faceValues, body.faceValuesKey);
    gl.uniformMatrix4fv(u.uModel, false, model(this.mat.model, body.quat, pos));
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, a.tex);
    gl.uniform2f(u.uMat, a.rough, a.metal);
    if (body.highlight) gl.uniform4f(u.uHighlight, this.accent[0], this.accent[1], this.accent[2], 0.6);
    else gl.uniform4f(u.uHighlight, 0, 0, 0, 0);
    this.drawMesh(this.mesh(body.shape));
  }

  drawGround(mode, extentX, extentZ, occN, strength) {
    const { gl } = this, u = this.ground.u;
    gl.useProgram(this.ground.p);
    gl.uniformMatrix4fv(u.uViewProj, false, this.mat.vp);
    gl.uniformMatrix4fv(u.uShadowMat, false, this.mat.shadow);
    gl.uniform2f(u.uExtent, extentX, extentZ);
    gl.uniform1i(u.uMode, mode);
    gl.uniform4fv(u.uOcc, this.occ);
    gl.uniform1i(u.uOccN, occN);
    gl.uniform2f(u.uStrength, strength[0], strength[1]);
    gl.enable(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);
    gl.bindVertexArray(this.quad);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.disable(gl.BLEND);
    gl.enable(gl.DEPTH_TEST);
  }

  // Shadow map of the table dice from the key light; redrawn only when a body moved.
  shadowPass(bodies, bounds) {
    const { gl } = this;
    let key = bounds.x * 31 + bounds.z * 17 + (bounds.minZ ?? -bounds.z) * 23 + (bounds.maxZ ?? bounds.z) * 29 + bodies.length * 1e4;
    bodies.forEach((b, i) => {
      if (b.dead) return;
      const p = b.pos, q = b.quat;
      key += (i + 1) * (p[0] * 1.3 + p[1] * 7.1 + p[2] * 3.7 + q[0] * 11 + q[1] * 13 + q[2] * 17 + q[3] * 19);
    });
    const lightKey = `${bounds.x},${bounds.minZ ?? -bounds.z},${bounds.maxZ ?? bounds.z}`;
    if (lightKey !== this.lightKey) {
      // Ortho light frustum around the table box (dice fly up to y ≈ 4).
      this.lightKey = lightKey;
      const view = lookAt(new Float32Array(16), KEY_DIR.map(c => c * 20), [0, 0, 0], [0, 0, -1]);
      const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
      for (const x of [-bounds.x - 1, bounds.x + 1]) for (const y of [0, 4]) for (const z of [(bounds.minZ ?? -bounds.z) - 1, (bounds.maxZ ?? bounds.z) + 1]) {
        for (let r = 0; r < 3; r++) {
          const v = view[r] * x + view[4 + r] * y + view[8 + r] * z + view[12 + r];
          lo[r] = Math.min(lo[r], v); hi[r] = Math.max(hi[r], v);
        }
      }
      // Far plane margin: the ground behind a die flying at y = 4 lies ≤ 5 units further along the light.
      mul(this.mat.light, ortho(this.mat.proj, lo[0], hi[0], lo[1], hi[1], -hi[2] - 1, -lo[2] + 3), view);
      mul(this.mat.shadow, SHADOW_BIAS, this.mat.light);
      this.shadowKey = NaN;
    }
    if (key === this.shadowKey) return;
    this.shadowKey = key;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.shadowFbo);
    gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
    gl.clear(gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.POLYGON_OFFSET_FILL);
    gl.polygonOffset(2, 4);
    gl.useProgram(this.depth.p);
    gl.uniformMatrix4fv(this.depth.u.uLightVP, false, this.mat.light);
    for (const b of bodies) {
      if (b.dead) continue;
      gl.uniformMatrix4fv(this.depth.u.uModel, false, model(this.mat.model, b.quat, b.pos));
      this.drawMesh(this.mesh(b.shape));
    }
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  // Draws the whole frame: table dice framed onto tableRect, then each tray preview { body, rect, clip? }.
  render(tableBodies, tableRect, bounds, previews = []) {
    const { gl, canvas } = this;
    if (!gl || gl.isContextLost()) return;
    const box = canvas.getBoundingClientRect(), dpr = Math.min(devicePixelRatio || 1, DPR_MAX);
    Object.assign(this.view, { left: box.left, top: box.top, width: box.width, height: box.height, dpr });
    const W = Math.max(1, Math.round(box.width * dpr)), H = Math.max(1, Math.round(box.height * dpr));
    if (canvas.width !== W) canvas.width = W;
    if (canvas.height !== H) canvas.height = H;
    const table = tableRect.width > 0 && tableRect.height > 0;
    gl.disable(gl.SCISSOR_TEST);
    gl.depthRange(0, 1);
    gl.depthMask(true);
    if (table) this.shadowPass(tableBodies, bounds);
    gl.viewport(0, 0, W, H);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    const env = this.env();
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, env.tex);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, this.shadowTex);
    const u = this.die.u, n = previews.length, slot = 1 / (n + 1);
    const visible = ({ body, rect, clip }) => !body.dead && rect.width > 0 && rect.height > 0 && this.scissor(clip);

    // All shadows first, so no camera's ground darkens another camera's dice. The dice then get one
    // depth slice per camera (table farthest, later previews nearer): cameras never depth-test
    // against each other and nothing needs a scissored depth clear.
    for (const p of previews) {
      if (!visible(p)) continue;
      const { y } = this.previewCamera(p.body, p.rect), r = p.body.shape.radius;
      this.occ.set([0, y, 0, this.mesh(p.body.shape).occ], 0);
      this.drawGround(1, r * 3.5, r * 3.5, 1, [0.45, 0.5]);
    }
    gl.disable(gl.SCISSOR_TEST);

    if (table) {
      const cam = this.camera(tableRect, bounds);
      this.mat.vp.set(cam.vp);
      this.frame(this.mat.vp, tableRect);
      let occN = 0;
      for (const b of tableBodies) {
        if (b.dead || occN >= MAX_OCC) continue;
        this.occ.set([b.pos[0], b.pos[1], b.pos[2], this.mesh(b.shape).occ], occN++ * 4);
      }
      // The ground quad covers the whole canvas (plus a margin), not just the physical bounds.
      const k = tableRect.height / (2 * cam.half);
      const cx = tableRect.left + tableRect.width / 2 - this.view.left, cy = tableRect.top + tableRect.height / 2 - this.view.top;
      this.drawGround(0, (Math.max(cx, box.width - cx) + 8) / k, (Math.max(cy, box.height - cy) + 8) / k, occN, [0.5, 0.5]);
      gl.depthRange(n * slot, 1);
      gl.useProgram(this.die.p);
      gl.uniformMatrix4fv(u.uViewProj, false, this.mat.vp);
      gl.uniformMatrix4fv(u.uShadowMat, false, this.mat.shadow);
      gl.uniform1i(u.uShadowOn, 1);
      gl.uniform3f(u.uCam, 0, cam.H, 0);
      this.setEnvMap(cx + this.view.left, cy + this.view.top, k, env.size);
      gl.uniform4fv(u.uEnvMap, this.envMap);
      for (const b of tableBodies) if (!b.dead) this.drawDie(b, b.pos);
    }

    previews.forEach((p, i) => {
      if (!visible(p)) return;
      const { body, rect } = p, { eye, y, dist } = this.previewCamera(body, rect);
      gl.depthRange((n - 1 - i) * slot, (n - i) * slot);
      gl.useProgram(this.die.p);
      gl.uniformMatrix4fv(u.uViewProj, false, this.mat.vp);
      gl.uniform1i(u.uShadowOn, 0);
      gl.uniform3fv(u.uCam, eye);
      this.setEnvMap(rect.left + rect.width / 2, rect.top + rect.height / 2, rect.height / (2 * dist * PREVIEW_TAN), env.size);
      gl.uniform4fv(u.uEnvMap, this.envMap);
      this.drawDie(body, [0, y, 0]);
    });
    gl.disable(gl.SCISSOR_TEST);
    gl.depthRange(0, 1);
    gl.bindVertexArray(null);
  }

  dispose() {
    const { gl, canvas } = this;
    canvas.removeEventListener('webglcontextlost', this.lost);
    canvas.removeEventListener('webglcontextrestored', this.restored);
    this.media.removeEventListener('change', this.themeChanged);
    if (gl && !gl.isContextLost()) {
      for (const a of this.atlases.values()) gl.deleteTexture(a.tex);
      for (const t of this.textures.values()) gl.deleteTexture(t.tex);
      for (const m of this.meshes.values()) gl.deleteVertexArray(m.vao);
      for (const p of [this.die, this.depth, this.ground]) gl.deleteProgram(p.p);
      gl.deleteTexture(this.shadowTex);
      gl.deleteFramebuffer(this.shadowFbo);
      gl.deleteVertexArray(this.quad);
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    }
    for (const e of this.images.values()) e.img.onload = null;
    this.atlases.clear();
    this.meshes.clear();
    this.textures.clear();
    this.gl = null;
    this.onChange = null;
  }
}
