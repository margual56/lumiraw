/** Does the gallery ranking group bursts and pick the right frames? */
import {
  DEFAULTS, blend, blinking, exposurePenalty, eyesPenalty, focusPenalty, groupBursts, keepCount,
  likeness, pick, relativeSharpness, shut, toCsv,
} from '../web/src/lib/cull.js';

let failures = 0;
const check = (what, ok) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};

// Features along a few directions: frames of one scene share a direction,
// with a little of their own.
const DIM = 16;
const feature = (scene, wobble = 0) => Float32Array.from({ length: DIM },
  (_, i) => 1 + (i === scene ? 5 : 0) + (i === (scene + 7) % DIM ? wobble : 0));
const item = (name, scene, captured, aesthetic, technical, more = {}) =>
  ({ name, captured, aesthetic, technical, feature: feature(scene, more.wobble ?? 0),
     blown: 0.002, crushed: 0.03, brightness: 0.45, sharpness: 100, ...more });
const names = (groups) => groups.map((g) => g.map((it) => it.name.replace(/\.\w+$/, '').replace(/\D/g, '')).join('+')).join(' ');
const levers = (over = {}) => ({ ...DEFAULTS, ...over });

const gallery = [
  item('IMG_0010.CR3', 1, 1000, 5.0, 6.0),
  item('IMG_0011.CR3', 1, 1001, 5.5, 4.0, { wobble: 0.3 }),
  item('IMG_0012.CR3', 1, 1002, 4.0, 3.0, { wobble: 0.5 }),
  item('IMG_0013.CR3', 2, 1003, 6.0, 6.0),          // straight after, another scene
  item('IMG_0014.CR3', 2, 1300, 6.1, 6.1),          // that scene again, minutes later
  item('IMG_0002.CR3', 3, 500, 3.0, 2.0),
];

// --- likeness ----------------------------------------------------------------

const like = likeness(gallery);
const centredCosine = (a, b) => {
  const mean = Array.from({ length: DIM }, (_, i) => gallery.reduce((s, it) => s + it.feature[i], 0) / gallery.length);
  const u = Array.from(a.feature, (v, i) => v - mean[i]);
  const v = Array.from(b.feature, (x, i) => x - mean[i]);
  const d = u.reduce((s, x, i) => s + x * v[i], 0);
  return d / Math.hypot(...u) / Math.hypot(...v);
};
check('likeness is the centred cosine, worked out the cheap way',
  gallery.every((a) => gallery.every((b) => Math.abs(like(a, b) - centredCosine(a, b)) < 1e-9)));
const cache = new WeakMap();
likeness(gallery, cache)(gallery[0], gallery[1]);
check('a pair once compared is kept', cache.get(gallery[0])?.has(gallery[1]));
const grown = [...gallery, item('x', 4, 0, 1, 1)];
check('the cached figure still holds once the gallery grows',
  Math.abs(likeness(grown, cache)(gallery[0], gallery[1]) - likeness(grown)(gallery[0], gallery[1])) < 1e-9);

// --- bursts --------------------------------------------------------------------

check(`a burst of one scene is one group (${names(groupBursts(gallery))})`,
  names(groupBursts(gallery)) === '0002 0010+0011+0012 0013 0014');

const untimed = gallery.map((it) => ({ ...it, captured: 0 }));
check(`without times, likeness alone groups, the same scene minutes apart too (${names(groupBursts(untimed))})`,
  names(groupBursts(untimed)) === '0002 0010+0011+0012 0013+0014');

const twins = [item('a.jpg', 1, 0, 5, 5), item('m.jpg', 2, 0, 5, 5), item('z-copy.jpg', 1, 0, 5, 5),
               item('q.jpg', 3, 0, 5, 5)];
check('a duplicate is found though it does not sit next to its twin',
  groupBursts(twins).some((g) => g.length === 2 && g.every((it) => it.feature[1] === 6)));

const interleaved = [item('1', 1, 1000, 5, 5), item('2', 2, 1001, 5, 5), item('3', 1, 1002, 5, 5),
                     item('4', 3, 1100, 5, 5)];
check(`a burst interrupted by one other frame is still one burst (${names(groupBursts(interleaved))})`,
  names(groupBursts(interleaved)) === '1+3 2 4');
check('the time window is a lever', names(groupBursts(interleaved, { gap: 1 })) === '1 2 3 4');

// --- marks -----------------------------------------------------------------------

check('blend weighs the two marks', Math.abs(blend(gallery[0], 0.25) - 5.75) < 1e-9);
check('a little clipping costs nothing',
  exposurePenalty({ blown: 0.004, crushed: 0.04 }, 1) === 0);
check('a burnt sky costs marks', Math.abs(exposurePenalty({ blown: 0.09, crushed: 0 }, 0.5) - 0.85) < 1e-9);
check('blown costs more than as much black',
  exposurePenalty({ blown: 0.2, crushed: 0 }, 1) > exposurePenalty({ blown: 0, crushed: 0.2 }, 1));
check('no photo loses more than three marks', exposurePenalty({ blown: 1, crushed: 1 }, 1) === 3);
check('the sharpest of a burst loses nothing', focusPenalty(80, 80, 1) === 0);
check('a quarter of the sharpness costs three marks at full strength',
  Math.abs(focusPenalty(20, 80, 1) - 3) < 1e-9);
check('focus off costs nothing', focusPenalty(1, 80, 0) === 0);

// --- picking -----------------------------------------------------------------------

check('a count is a count', keepCount(40, { mode: 'count', value: 5 }) === 5);
check('a share rounds up and keeps at least one',
  keepCount(40, { mode: 'percent', value: 10 }) === 4 && keepCount(3, { mode: 'percent', value: 1 }) === 1);

const groups = groupBursts(gallery);
const two = { mode: 'count', value: 2 };
const technical = pick(groups, levers({ weight: 0, focus: 0, variety: 0 }), two, gallery.length);
check('the best-marked of the burst leads it when focus is off',
  technical.groups.find((g) => g.length === 3)[0].name === 'IMG_0010.CR3');

// The same burst, but the frame the networks like best is the softest.
const soft = [item('b1', 1, 1000, 5.6, 5.0, { sharpness: 25 }), item('b2', 1, 1001, 5.4, 4.9, { sharpness: 100 })];
check('in a burst the sharp frame beats a slightly better-liked soft one',
  pick([soft], levers({ focus: 0.5 }), two, 2).groups[0][0].name === 'b2');
check('with focus off the better-liked frame leads',
  pick([soft], levers({ focus: 0 }), two, 2).groups[0][0].name === 'b1');

const burnt = [[item('clean', 1, 0, 5.0, 5.0)], [item('burnt', 2, 0, 5.3, 5.3, { blown: 0.15 })]];
check('a burnt photo falls below a clean one when faults count',
  pick(burnt, levers({ faults: 0.5 }), { mode: 'count', value: 1 }, 2).picked.has(burnt[0][0]));
check('and leads when they do not',
  pick(burnt, levers({ faults: 0 }), { mode: 'count', value: 1 }, 2).picked.has(burnt[1][0]));

// Three separate moments of one scene, then something else a little worse.
const scenes = [item('s1', 1, 100, 6.0, 6.0), item('s2', 1, 400, 5.9, 5.9, { wobble: 0.2 }),
                item('s3', 1, 700, 5.8, 5.8, { wobble: 0.4 }), item('other', 5, 900, 5.5, 5.5),
                item('third', 9, 950, 4.0, 4.0)];
const sceneGroups = groupBursts(scenes);
const sceneLike = likeness(scenes);
const chosen = (variety) => [...pick(sceneGroups, levers({ variety }), two, scenes.length, sceneLike).picked]
  .map((it) => it.name).sort().join();
check(`without variety the two best are the same scene (${chosen(0)})`, chosen(0) === 's1,s2');
check(`with variety the second pick is something else (${chosen(0.5)})`, chosen(0.5) === 'other,s1');

const order = pick(sceneGroups, levers({ variety: 0.5 }), two, scenes.length, sceneLike).groups
  .map((g) => g[0].name);
check(`picks come first, in the order they were made (${order.join()})`,
  order[0] === 's1' && order[1] === 'other' && order.length === 5);

// --- faces -------------------------------------------------------------------------

const face = (open, sharpness = 50) => ({ box: [0.3, 0.3, 0.6, 0.6], eyes: [open, open], sharpness });
check('open eyes are not shut', shut(face(0.27)) === 0);
check('shut eyes are', shut(face(0.04)) === 1);
check('a squint is neither', shut(face(0.12)) > 0 && shut(face(0.12)) < 1);
check('a face too small to read says nothing', shut({ eyes: null }) === null);
check('a portrait with its eyes shut loses three marks at full strength',
  eyesPenalty({ faces: [face(0.03)] }, 1) === 3);
check('one blink in a group of four costs less, and something',
  ((p) => p > 0 && p < 3)(eyesPenalty({ faces: [face(0.03), face(0.27), face(0.27), face(0.27)] }, 1)));
check('no faces, no penalty', eyesPenalty({}, 1) === 0 && eyesPenalty({ faces: [] }, 1) === 0);
check('blinking counts the shut faces',
  blinking({ faces: [face(0.03), face(0.27), { eyes: null }] }) === 1);

const blink = [item('blink', 1, 1000, 5.6, 5.2, { faces: [face(0.03)] }),
               item('open', 1, 1001, 5.3, 5.0, { faces: [face(0.28)] })];
check('in a burst the frame with open eyes wins',
  pick([blink], levers(), two, 2).groups[0][0].name === 'open');
check('unless shut eyes do not count',
  pick([blink], levers({ eyes: 0, focus: 0 }), two, 2).groups[0][0].name === 'blink');

// The whole frame is sharpest in the first, the face in the second: focus
// was on the background.
const missed = [item('background', 1, 1000, 5.4, 5.2, { sharpness: 200, faces: [face(0.27, 20)] }),
                item('face', 1, 1001, 5.3, 5.1, { sharpness: 80, faces: [face(0.27, 90)] })];
check('in a burst of faces, the sharp face wins over the sharp background',
  pick([missed], levers({ focus: 0.5 }), two, 2).groups[0][0].name === 'face');

// --- sharpness and CSV ---------------------------------------------------------------

const sharp = relativeSharpness(gallery.map((it, i) => ({ ...it, sharpness: 100 + i })));
check('relative sharpness tops out at 100', Math.max(...sharp.values()) === 100);
const crowd = Array.from({ length: 30 }, (_, i) => ({ ...gallery[0], sharpness: 100 + i }));
const freak = [...crowd, { ...gallery[0], sharpness: 1e6 }];
check('in a big gallery one freak does not flatten the rest',
  relativeSharpness(freak).get(crowd[29]) === 100 && relativeSharpness(freak).get(crowd[0]) > 70);

const result = pick(groups, levers(), two, gallery.length, like);
const csv = toCsv(result, result.picked, levers()).trim().split('\n');
const header = csv[0].split(',');
check('the CSV has a header and a row per photograph', csv.length === gallery.length + 1);
check('the CSV carries the penalties and the exposure',
  ['exposure_penalty', 'eyes_penalty', 'focus_penalty', 'score', 'blown_pct', 'black_pct',
   'sharpness', 'faces', 'eyes_shut']
    .every((k) => header.includes(k)));
check('the CSV starts with the picks', csv[1].endsWith(',1') && csv[2].endsWith(',1'));
check('names with commas are quoted',
  toCsv(pick([[{ ...gallery[0], name: 'a, "b".jpg' }]], levers(), two, 1), new Set(), levers())
    .includes('"a, ""b"".jpg"'));

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
