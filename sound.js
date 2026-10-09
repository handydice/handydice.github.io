// Web Audio instead of <audio>: iOS starts <audio> noticeably late. Buffers are decoded up front
// so a sound plays the moment it is triggered; a sound that failed to load stays silent.
let context;
const buffers = {};
// click (UI) is the die-on-die contact sample at half volume.
const files = { throw: 'throw.mp3', shuffle: 'shuffle.mp3', click: 'contact-die.wav', table: 'contact-table.wav', die: 'contact-die.wav' };
const volume = { click: 0.5 };
let voices = 0, windowStart = 0;

export function loadSounds() {
  if (context) return;
  context = new AudioContext();
  for (const [name, file] of Object.entries(files))
    fetch(`sounds/${file}`).then(r => r.arrayBuffer()).then(b => context.decodeAudioData(b)).then(b => { buffers[name] = b; }).catch(() => {});
}

// iOS only releases audio after a user gesture.
export const resumeSounds = () => context?.resume();

function play(name, gain, rate = 1) {
  if (!buffers[name]) return;
  context.resume();
  const source = context.createBufferSource(), node = context.createGain();
  source.buffer = buffers[name];
  source.playbackRate.value = rate;
  node.gain.value = gain;
  source.connect(node).connect(context.destination);
  source.start();
}

export const playSound = name => play(name, volume[name] ?? 1);

// 3D contacts, triggered by physics3d.js: type 'table' | 'die', speed = closing velocity in world units/s.
// Loudness rises with speed³: a die dropped from the hand (~8.5) is quiet, only fast and cup throws (16+)
// reach full volume. A slight pitch wobble keeps repeats from sounding identical.
export function playImpact(type, speed) {
  const now = performance.now();
  if (now - windowStart > 25) { windowStart = now; voices = 0; }
  if (voices >= 8) return; // pileups stay a thud instead of a crackle
  voices++;
  const k = Math.min(1, speed / 16) ** 3;
  play(type, type === 'die' ? Math.min(1.2, 3.4 * k) : 0.95 * k, 1 + (Math.random() - 0.5) * 0.08); // die sample is ~13 dB quieter
}
