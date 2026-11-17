/** Does the slicer cut where it says it will? */
import { MAX_SLICES, cuts, outputFor, stripName, strips } from '../web/src/lib/slice.js';

let failures = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${ok || !detail ? '' : `: ${detail}`}`);
  if (!ok) failures += 1;
};
const show = (v) => JSON.stringify(v);

/** Strips that cover the whole side, in order, with no gap, overlap or empty one. */
const tiles = (pairs, length) =>
  pairs.length > 0 && pairs[0][0] === 0 && pairs[pairs.length - 1][1] === length
  && pairs.every(([a, b], i) => b > a && (i === 0 || pairs[i - 1][1] === a));

// --- a number of strips -----------------------------------------------------

check('three even strips of 3000', show(cuts(3000, { by: 'count', count: 3 }))
  === show([[0, 1000], [1000, 2000], [2000, 3000]]));

for (const [length, count] of [[6000, 7], [5456, 3], [1001, 10], [7, 7], [4000, 13]]) {
  const pairs = cuts(length, { by: 'count', count });
  const widths = pairs.map(([a, b]) => b - a);
  check(`${count} strips of ${length} cover it`, tiles(pairs, length), show(pairs));
  check(`${count} strips of ${length} differ by a pixel at most`,
    Math.max(...widths) - Math.min(...widths) <= 1, show(widths));
  check(`${count} strips of ${length} are ${count}`, pairs.length === count);
}

check('more strips than pixels gives one a pixel', cuts(5, { by: 'count', count: 9 }).length === 5);
check('one strip is the whole side', show(cuts(800, { by: 'count', count: 1 })) === show([[0, 800]]));
check('never more than the most', cuts(9000, { by: 'count', count: 500 }).length === MAX_SLICES);

// --- a size of strip --------------------------------------------------------

check('1080 a strip of 3000 leaves 840 for the last',
  show(cuts(3000, { by: 'size', size: 1080 })) === show([[0, 1080], [1080, 2160], [2160, 3000]]));
check('a size that divides it leaves no sliver',
  show(cuts(3000, { by: 'size', size: 1000 })) === show([[0, 1000], [1000, 2000], [2000, 3000]]));
check('a size past the side is one strip', show(cuts(800, { by: 'size', size: 5000 })) === show([[0, 800]]));
for (const [length, size] of [[6000, 1080], [5456, 1], [4000, 333]]) {
  const pairs = cuts(length, { by: 'size', size });
  check(`${size} px strips of ${length} cover it`, tiles(pairs, length), show(pairs.slice(-2)));
  check(`${size} px strips of ${length} stay within the most`, pairs.length <= MAX_SLICES);
}
check('a tiny size is raised, not obeyed',
  cuts(5456, { by: 'size', size: 1 }).every(([a, b], i, all) => i === all.length - 1 || b - a === 110));

// --- the rectangles, the names, the format ----------------------------------

check('columns are side by side', show(strips(300, 200, { across: 'columns', by: 'count', count: 3 }))
  === show([{ x: 0, y: 0, width: 100, height: 200 }, { x: 100, y: 0, width: 100, height: 200 },
            { x: 200, y: 0, width: 100, height: 200 }]));
check('rows are one above the other', show(strips(300, 200, { across: 'rows', by: 'size', size: 150 }))
  === show([{ x: 0, y: 0, width: 300, height: 150 }, { x: 0, y: 150, width: 300, height: 50 }]));
check('names sort in order', stripName('DSC05825.ARW', 2, 12, 'jpg') === 'DSC05825_03-of-12.jpg');
check('a nameless picture is still named', stripName('', 0, 3, 'png') === 'photo_1-of-3.png');
check('PNG stays PNG', outputFor('image/png').ext === 'png');
check('WebP only where it can be written', outputFor('image/webp').ext === 'jpg'
  && outputFor('image/webp', { webp: true }).ext === 'webp');
check('anything else is written as JPEG', outputFor('image/avif').type === 'image/jpeg');

if (failures) {
  console.error(`${failures} check(s) failed`);
  process.exit(1);
}
