/** Does the gallery ranking group bursts and pick the right frames? */
import { blend, centred, groupBursts, keepCount, pick, toCsv } from '../web/src/lib/cull.js';

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
const item = (name, scene, captured, aesthetic, technical, wobble = 0) =>
  ({ name, captured, aesthetic, technical, feature: feature(scene, wobble) });

const gallery = [
  item('IMG_0010.CR3', 1, 1000, 5.0, 6.0),
  item('IMG_0011.CR3', 1, 1001, 5.5, 4.0, 0.3),
  item('IMG_0012.CR3', 1, 1002, 4.0, 3.0, 0.5),
  item('IMG_0013.CR3', 2, 1003, 6.0, 6.0),          // straight after, another scene
  item('IMG_0014.CR3', 2, 1300, 6.1, 6.1),          // that scene again, minutes later
  item('IMG_0002.CR3', 3, 500, 3.0, 2.0),
];

check('blend weighs the two marks', Math.abs(blend(gallery[0], 0.25) - 5.75) < 1e-9);

const units = centred(gallery);
check('centred features have unit length',
  [...units.values()].every((v) => Math.abs(Math.hypot(...v) - 1) < 1e-5));

const groups = groupBursts(gallery, { gap: 4, similarity: 0.8 });
const names = groups.map((g) => g.map((it) => it.name.slice(4, 8)).join('+'));
check(`a burst of one scene is one group (${names.join(' ')})`,
  names.join(' ') === '0002 0010+0011+0012 0013 0014');

const untimed = gallery.map((it) => ({ ...it, captured: 0 }));
const loose = groupBursts(untimed, { similarity: 0.8 });
check('without times, likeness alone groups, in name order',
  loose.map((g) => g.length).join(',') === '1,3,2');

check('a count is a count', keepCount(40, { mode: 'count', value: 5 }) === 5);
check('a share rounds up and keeps at least one',
  keepCount(40, { mode: 'percent', value: 10 }) === 4 && keepCount(3, { mode: 'percent', value: 1 }) === 1);

const technical = pick(groups, 0, { mode: 'count', value: 2 }, gallery.length);
check('the sharpest of the burst leads it when the technical mark rules',
  technical.groups[1][0].name === 'IMG_0010.CR3');
const aesthetic = pick(groups, 1, { mode: 'count', value: 2 }, gallery.length);
check('the best-looking of the burst leads it when the aesthetic mark rules',
  aesthetic.groups[1][0].name === 'IMG_0011.CR3');
check('one pick per group at most, the best groups first',
  [...aesthetic.picked].map((it) => it.name).sort().join() === 'IMG_0013.CR3,IMG_0014.CR3');

const csv = toCsv(groups, aesthetic.picked, 1).trim().split('\n');
check('the CSV has a header and a row per photograph', csv.length === gallery.length + 1);
check('the CSV is best first', csv[1].startsWith('IMG_0014.CR3,'));
check('names with commas are quoted',
  toCsv([[{ ...gallery[0], name: 'a, "b".jpg' }]], new Set(), 0.5).includes('"a, ""b"".jpg"'));

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
