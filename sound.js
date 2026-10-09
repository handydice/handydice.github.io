// Web Audio instead of <audio>: iOS starts <audio> noticeably late. Buffers are decoded up front
// so a sound plays the moment it is triggered; a sound that failed to load stays silent.
let context;
const buffers = {};
// click is the die-on-die contact sample, at half volume as a UI sound.
const files = { throw: 'throw.mp3', shuffle: 'shuffle.mp3', cup: 'cup.mp3', click: 'contact-die.wav' };
const volume = { click: 0.5 };

export function loadSounds() {
  if (context) return;
  context = new AudioContext();
  for (const [name, file] of Object.entries(files))
    fetch(`sounds/${file}`).then(r => r.arrayBuffer()).then(b => context.decodeAudioData(b)).then(b => { buffers[name] = b; }).catch(() => {});
}

// iOS only releases audio after a user gesture.
export const resumeSounds = () => context?.resume();

export function playSound(name) {
  if (!buffers[name]) return;
  context.resume();
  const source = context.createBufferSource();
  source.buffer = buffers[name];
  const gain = context.createGain();
  gain.gain.value = volume[name] ?? 1;
  source.connect(gain).connect(context.destination);
  source.start();
}
