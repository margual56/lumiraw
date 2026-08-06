/** Does the crop overlay's geometry hold its anchor? */
import { MIN, cursor, drawn, grip, moved, resized, same } from '../web/src/lib/crop.js';

let failures = 0;
const check = (what, ok) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};

const near = (a, b, tol = 1e-9) =>
  a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) <= tol);
const show = (r) => `[${r.map((v) => v.toFixed(4)).join(', ')}]`;

/** A 3:2 picture, 6000x4000, as the viewer converts a ratio for these functions. */
const SQUARE = 6000 / (1 * 4000);     // 1:1 on a 3:2 frame
const WIDE = 6000 / (1.77778 * 4000); // 16:9 on a 3:2 frame

// --- the anchor stays put, which is the whole point -------------------------

// Free-hand, dragged up and to the left: the pressed point is the box's own
// bottom-right corner afterwards.
check('a free drag keeps the pressed corner',
  near(drawn([0.8, 0.9], [0.3, 0.4], 0), [0.3, 0.4, 0.5, 0.5]));

// The bug this replaces: with a ratio locked, the old code wrote the pointer's
// y as the box's top and then derived the height downward from it, so the
// pressed corner slid vertically the whole time you dragged.
for (const [name, aspect] of [['a square', SQUARE], ['16:9', WIDE]]) {
  for (const [to, label] of [[[0.3, 0.4], 'up and left'], [[0.9, 0.2], 'up and right'],
                             [[0.1, 0.8], 'down and left'], [[0.95, 0.9], 'down and right']]) {
    const anchor = [0.5, 0.5];
    const box = drawn(anchor, to, aspect);
    const corners = [[box[0], box[1]], [box[0] + box[2], box[1]],
                     [box[0], box[1] + box[3]], [box[0] + box[2], box[1] + box[3]]];
    check(`${name} dragged ${label} still owns the pressed corner ${show(box)}`,
      corners.some((c) => near(c, anchor, 1e-9)));
  }
}

check('a locked drag comes out at the locked shape',
  Math.abs(drawn([0.5, 0.5], [0.9, 0.6], SQUARE)[3]
           / drawn([0.5, 0.5], [0.9, 0.6], SQUARE)[2] - SQUARE) < 1e-9);

// A sideways drag under a lock must still open a box rather than a sliver,
// otherwise the ratios in the dropdown are unusable with a flick of the wrist.
check('a mostly sideways locked drag opens a box the width of the drag',
  Math.abs(drawn([0.1, 0.1], [0.6, 0.12], SQUARE)[2] - 0.5) < 1e-9);

// The only thing that may cut it back is the frame.
check('and when the frame is what limits it, it takes all the room there is',
  (() => { const r = drawn([0.1, 0.5], [0.6, 0.52], SQUARE);
           return Math.abs(r[3] - 0.5) < 1e-9 && Math.abs(r[1] - 0.5) < 1e-9; })());

// --- the frame cuts the box back, it does not shove it ----------------------

const cramped = drawn([0.2, 0.8], [0.9, 0.95], SQUARE);
check(`a locked drag into the bottom edge shrinks instead of sliding ${show(cramped)}`,
  near([cramped[0], cramped[1]], [0.2, 0.8]) && cramped[1] + cramped[3] <= 1 + 1e-9);
check('and it fills the room it does have',
  Math.abs(cramped[1] + cramped[3] - 1) < 1e-9);

for (const anchor of [[0, 0], [1, 1], [0, 1], [1, 0], [0.5, 0.02]]) {
  for (const to of [[0, 0], [1, 1], [0.5, 0.5], [0.02, 0.98]]) {
    const box = drawn(anchor, to, SQUARE);
    check(`a locked drag from ${show(anchor)} to ${show(to)} stays inside the frame ${show(box)}`,
      box[0] >= -1e-9 && box[1] >= -1e-9
      && box[0] + box[2] <= 1 + 1e-9 && box[1] + box[3] <= 1 + 1e-9
      && box[2] >= 0 && box[3] >= 0);
  }
}

// --- resizing by a handle --------------------------------------------------

const BOX = [0.2, 0.3, 0.4, 0.4];   // x, y, w, h

check('dragging the north-west corner leaves the south-east one alone',
  near(resized(BOX, 'nw', [0.1, 0.1], 0), [0.1, 0.1, 0.5, 0.6]));
check('dragging the south-east corner leaves the north-west one alone',
  near(resized(BOX, 'se', [0.9, 0.8], 0), [0.2, 0.3, 0.7, 0.5]));
check('dragging the east edge moves only the east edge',
  near(resized(BOX, 'e', [0.75, 0.05], 0), [0.2, 0.3, 0.55, 0.4]));
check('dragging the north edge moves only the north edge',
  near(resized(BOX, 'n', [0.95, 0.15], 0), [0.2, 0.15, 0.4, 0.55]));
check('dragging a handle past its anchor turns the box inside out, not negative',
  (() => { const r = resized(BOX, 'e', [0.05, 0.5], 0);
           return near(r, [0.05, 0.3, 0.15, 0.4]); })());
check('a locked edge drag keeps the opposite edge and the shape',
  (() => { const r = resized(BOX, 'e', [0.7, 0.9], SQUARE);
           return Math.abs(r[0] - 0.2) < 1e-9 && Math.abs(r[1] - 0.3) < 1e-9
                  && Math.abs(r[3] / r[2] - SQUARE) < 1e-9; })());
check('a locked corner drag keeps the opposite corner and the shape',
  (() => { const r = resized(BOX, 'nw', [0.05, 0.05], SQUARE);
           return Math.abs(r[0] + r[2] - 0.6) < 1e-9 && Math.abs(r[1] + r[3] - 0.7) < 1e-9
                  && Math.abs(r[3] / r[2] - SQUARE) < 1e-9; })());

// --- sliding a selection, which is what could not be done at all -----------

check('sliding keeps the size',
  near(moved(BOX, 0.1, -0.1), [0.3, 0.2, 0.4, 0.4]));
check('sliding into a corner stops, it does not squash',
  near(moved(BOX, -0.9, -0.9), [0, 0, 0.4, 0.4]));
check('sliding into the far corner stops at the far corner',
  near(moved(BOX, 0.9, 0.9), [0.6, 0.6, 0.4, 0.4]));
check('a box as big as the frame cannot slide anywhere',
  near(moved([0, 0, 1, 1], 0.3, -0.2), [0, 0, 1, 1]));

// --- knowing what the pointer is over --------------------------------------

const TOL = [0.02, 0.03];
check('the middle of the box slides it', grip(BOX, [0.4, 0.5], TOL) === 'move');
check('a corner is a corner', grip(BOX, [0.2, 0.3], TOL) === 'nw');
check('the opposite corner too', grip(BOX, [0.6, 0.7], TOL) === 'se');
check('the other diagonal is named the other way round',
  grip(BOX, [0.6, 0.3], TOL) === 'ne' && grip(BOX, [0.2, 0.7], TOL) === 'sw');
check('the middle of an edge is that edge',
  grip(BOX, [0.6, 0.5], TOL) === 'e' && grip(BOX, [0.4, 0.3], TOL) === 'n');
check('just outside the box is nothing, so a press starts again',
  grip(BOX, [0.05, 0.05], TOL) === null);
check('just outside an edge still grips it, since aiming is not free',
  grip(BOX, [0.605, 0.5], TOL) === 'e');
check('there is no box to grip before one is drawn', grip(null, [0.4, 0.5], TOL) === null);
check('a thin box still has an inside to grab',
  grip([0.2, 0.3, 0.02, 0.02], [0.21, 0.31], TOL) === 'move');

check('the cursor says what a press would do',
  cursor(null) === 'crosshair' && cursor('move') === 'move'
  && cursor('n') === 'ns-resize' && cursor('w') === 'ew-resize'
  && cursor('nw') === 'nwse-resize' && cursor('se') === 'nwse-resize'
  && cursor('ne') === 'nesw-resize' && cursor('sw') === 'nesw-resize');

// --- a press that changed nothing is not an edit ----------------------------

check('a box is the same as itself', same(BOX, [...BOX]));
check('a box a hair away is still the same box', same(BOX, [0.2, 0.3, 0.4, 0.4 + 1e-9]));
check('a box that actually moved is not', !same(BOX, moved(BOX, 0.001, 0)));
check('nothing is not the same as a box', !same(null, BOX) && !same(BOX, null));

check('the minimum is a hundredth of the frame', MIN === 0.01);

process.exit(failures ? 1 : 0);
