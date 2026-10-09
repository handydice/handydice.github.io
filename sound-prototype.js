// sound-prototype.js — PROTOTYPE (branch audio-prototype), not for main: physics-triggered contact
// sounds. Question under test: do short per-impact samples, timed by the simulation, sync well enough?
// One contact = one short sample. Volume from the closing speed, slight pitch wobble against sameness.
let context;
const buffers = {};
const ids = new Map(), pairs = new Map();
let voices = 0, windowStart = 0;

export function loadContacts() {
  if (context) return;
  context = new AudioContext();
  for (const name of ['contact-table', 'contact-die'])
    fetch(`sounds/${name}.wav`).then(r => r.arrayBuffer()).then(b => context.decodeAudioData(b)).then(b => { buffers[name] = b; }).catch(() => {});
}

// iOS only releases audio after a user gesture.
export const resumeContacts = () => context?.resume();

const idOf = body => {
  let id = ids.get(body);
  if (id === undefined) ids.set(body, id = ids.size);
  return id;
};

// Loudness ∝ impact energy (speed²): slow topples and slides stay near-silent, only fast impacts
// sound loud. Thrown dice land around 12–18 world units/s, gentle flicks around 5–8.
const gainOf = (type, speed) => {
  const k = Math.min(1, speed / 16) ** 2;
  return type === 'die' ? Math.min(1.2, 3.4 * k) : 0.95 * k; // die sample is ~13 dB quieter
};

export function impact(type, speed, a, b) {
  const buffer = buffers[type === 'die' ? 'contact-die' : 'contact-table'];
  if (!buffer) return;
  context.resume();
  const now = performance.now();
  if (now - windowStart > 25) { windowStart = now; voices = 0; }
  if (voices >= 8) return; // worst-case pileups stay a thud instead of a crackle
  if (type === 'die') {
    const key = idOf(a) * 4096 + idOf(b), last = pairs.get(key) ?? -1e9;
    if (now - last < 40) return; // one sound per pair per rattle, not per manifold contact point
    pairs.set(key, now);
  }
  const g = gainOf(type, speed);
  const source = context.createBufferSource();
  source.buffer = buffer;
  source.playbackRate.value = 1 + (Math.random() - 0.5) * 0.08;
  const gain = context.createGain();
  gain.gain.value = g;
  source.connect(gain).connect(context.destination);
  const t = context.currentTime;
  source.start(t);
  gain.gain.setTargetAtTime(0, t + buffer.duration, 0.006); // fade the tail so repeated stops never click
  const log = globalThis.__contacts ??= [];
  if (log.length < 300) log.push({ type, speed: Math.round(speed * 10) / 10, gain: Math.round(g * 100) / 100 });
}
