// Web Audio instead of <audio>: iOS starts <audio> noticeably late. Buffers are decoded up front
// so a sound plays the moment it is triggered; a sound that failed to load stays silent.
let context;
const buffers = {};

export function loadSounds() {
  if (context) return;
  context = new AudioContext();
  for (const name of ['throw', 'shuffle', 'click'])
    fetch(`sounds/${name}.mp3`).then(r => r.arrayBuffer()).then(b => context.decodeAudioData(b)).then(b => { buffers[name] = b; }).catch(() => {});
}

// iOS only releases audio after a user gesture.
export const resumeSounds = () => context?.resume();

export function playSound(name) {
  if (!buffers[name]) return;
  context.resume();
  const source = context.createBufferSource();
  source.buffer = buffers[name];
  source.connect(context.destination);
  source.start();
}
