// Story dice inspired by the classic nine-die picture set: each die has six fixed motifs (forum order).
// Art in icons/story/. Lucide @70562c1e (icons/story/LICENSE), recolored #171717 and sized 256:
// house, lightbulb, speech=message-circle, clock, lock, flame, magnet, message=id-card, pyramid,
// tower=chess-rook, rainbow, tree=tree-deciduous, eye, earth, plane, smile=face-slightly-smiling,
// building, flashlight, apple, key, question=circle-question-mark, tent, frown=face-slightly-frowning,
// beetle=bug, die=dice-5, magnifier=search, hand, turtle, masks=drama, fish, sunflower=flower-2,
// book=book-open, bridge, moon, wand=wand-sparkles, lightning=zap, scales=scale, direction=arrow-down-right.
// Drawn for this project in the same outline style: sleep, arrow, footprint, sheep, learner,
// shooting-star, spinning-object (deliberately neutral disc), shadow, parachute, keyhole, arrows,
// abacus, bee, alien, phone (keypad), cane.
export const STORY_DICE = Object.freeze({
  'classic-1': ['house', 'lightbulb', 'sleep', 'speech', 'clock', 'arrow'],
  'classic-2': ['lock', 'footprint', 'flame', 'sheep', 'magnet', 'learner'],
  'classic-3': ['message', 'pyramid', 'tower', 'rainbow', 'tree', 'eye'],
  'classic-4': ['earth', 'plane', 'smile', 'building', 'flashlight', 'apple'],
  'classic-5': ['key', 'shooting-star', 'question', 'tent', 'spinning-object', 'frown'],
  'classic-6': ['beetle', 'die', 'magnifier', 'shadow', 'hand', 'turtle'],
  'classic-7': ['masks', 'fish', 'parachute', 'keyhole', 'sunflower', 'arrows'],
  'classic-8': ['book', 'bridge', 'abacus', 'moon', 'wand', 'bee'],
  'classic-9': ['alien', 'lightning', 'phone', 'scales', 'direction', 'cane'],
});

// Labels are English t() keys.
const LABELS = {
  house: 'House', lightbulb: 'Light bulb', sleep: 'Sleeping face', speech: 'Speech bubble', clock: 'Clock', arrow: 'Feathered arrow',
  lock: 'Padlock', footprint: 'Footprint', flame: 'Flame', sheep: 'Sheep', magnet: 'Magnet', learner: 'Learner plate',
  message: 'Message card', pyramid: 'Pyramid', tower: 'Tower', rainbow: 'Rainbow', tree: 'Tree', eye: 'Eye',
  earth: 'Globe', plane: 'Airplane', smile: 'Smiling face', building: 'Skyscraper', flashlight: 'Flashlight', apple: 'Apple',
  key: 'Key', 'shooting-star': 'Shooting star', question: 'Question mark', tent: 'Tent', 'spinning-object': 'Spinning object', frown: 'Sad face',
  beetle: 'Beetle', die: 'Die', magnifier: 'Magnifying glass', shadow: 'Shadow', hand: 'Hand', turtle: 'Turtle',
  masks: 'Theatre masks', fish: 'Fish', parachute: 'Parachute', keyhole: 'Keyhole', sunflower: 'Sunflower', arrows: 'Outward arrows',
  book: 'Book', bridge: 'Bridge', abacus: 'Abacus', moon: 'Crescent moon', wand: 'Magic wand', bee: 'Bee',
  alien: 'Alien', lightning: 'Lightning', phone: 'Mobile phone', scales: 'Scales', direction: 'Direction arrow', cane: 'Walking stick',
};

export const STORY_MOTIFS = Object.freeze(Object.fromEntries(Object.entries(LABELS)
  .map(([id, label]) => [id, Object.freeze({ label, src: `icons/story/${id}.svg` })])));

export const isStory = id => typeof id === 'string' && Object.hasOwn(STORY_DICE, id);

// Motif on side `value` (1..6) of story die `story`; undefined for anything invalid.
export function storyFace(story, value) {
  if (!isStory(story) || !Number.isInteger(value) || value < 1 || value > 6) return undefined;
  return STORY_MOTIFS[STORY_DICE[story][value - 1]];
}
