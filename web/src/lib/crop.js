/** The geometry of the selection rectangle, with no browser in it. */

/** Smallest selection worth keeping, as a fraction of each side. */
export const MIN = 0.01;

/** How close to an edge counts as grabbing it, in screen pixels. */
export const GRIP = 11;

const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);

/** Whether an aspect lock is in force. `0`, `null`, `NaN` and a negative all
 *  mean free, so a caller can pass whatever its ratio arithmetic produced. */
const locked = (aspect) => Number.isFinite(aspect) && aspect > 0;

/** A box with one corner pinned at `anchor`, growing `sx`/`sy` from it. */
function pinned([ax, ay], w, h, sx, sy, aspect) {
  if (locked(aspect)) {
    w = Math.max(w, h / aspect);
    w = Math.min(w, sx < 0 ? ax : 1 - ax, (sy < 0 ? ay : 1 - ay) / aspect);
    h = w * aspect;
  }
  return [sx < 0 ? ax - w : ax, sy < 0 ? ay - h : ay, w, h];
}

/** Which way the pointer went from the anchor. Dead on it counts as forward,
 *  so a box opens in a predictable direction instead of by a rounding error. */
const way = (delta) => (delta < 0 ? -1 : 1);

/** A fresh selection swept from `anchor` to the pointer. */
export function drawn(anchor, [px, py], aspect) {
  const [dx, dy] = [px - anchor[0], py - anchor[1]];
  return pinned(anchor, Math.abs(dx), Math.abs(dy), way(dx), way(dy), aspect);
}

/** The same selection with one handle moved to the pointer. */
export function resized([x, y, w, h], handle, [px, py], aspect) {
  const [ax, ay] = [handle.includes('w') ? x + w : x, handle.includes('n') ? y + h : y];
  const sideways = handle.includes('e') || handle.includes('w');
  const upright = handle.includes('n') || handle.includes('s');

  if (sideways && upright) return drawn([ax, ay], [px, py], aspect);
  // An edge under a lock leads with the axis it was dragged along and lets the
  // other follow, which is why one of the two sizes below is zero.
  if (locked(aspect)) {
    return sideways
      ? pinned([ax, ay], Math.abs(px - ax), 0, way(px - ax), 1, aspect)
      : pinned([ax, ay], 0, Math.abs(py - ay), 1, way(py - ay), aspect);
  }
  return sideways
    ? [Math.min(px, ax), y, Math.abs(px - ax), h]
    : [x, Math.min(py, ay), w, Math.abs(py - ay)];
}

/** The same selection, same size, slid by `dx`/`dy` and kept inside the frame. */
export function moved([x, y, w, h], dx, dy) {
  return [clamp(x + dx, 0, Math.max(0, 1 - w)), clamp(y + dy, 0, Math.max(0, 1 - h)), w, h];
}

/**
 * What the pointer is over: a handle name, `'move'` for the inside, or null for
 * the picture outside the box, where a press starts a new selection.
 */
export function grip(rect, [px, py], [tolX, tolY]) {
  if (!rect) return null;
  const [x, y, w, h] = rect;
  if (px < x - tolX || px > x + w + tolX || py < y - tolY || py > y + h + tolY) return null;

  // A grip may not swallow the box. On a thin crop the zones give way so there
  // is always an inside left to grab, which is the only way to slide one.
  const [tx, ty] = [Math.min(tolX, w / 3), Math.min(tolY, h / 3)];
  const west = Math.abs(px - x) <= tx;
  const east = !west && Math.abs(px - (x + w)) <= tx;
  const north = Math.abs(py - y) <= ty;
  const south = !north && Math.abs(py - (y + h)) <= ty;

  const handle = (north ? 'n' : south ? 's' : '') + (west ? 'w' : east ? 'e' : '');
  if (handle) return handle;
  return px >= x && px <= x + w && py >= y && py <= y + h ? 'move' : null;
}

/** The pointer shape that says what a press would do. */
export function cursor(handle) {
  if (!handle) return 'crosshair';
  if (handle === 'move') return 'move';
  if (handle === 'n' || handle === 's') return 'ns-resize';
  if (handle === 'e' || handle === 'w') return 'ew-resize';
  return handle === 'nw' || handle === 'se' ? 'nwse-resize' : 'nesw-resize';
}

/** Whether two rectangles are the same box, allowing for float arithmetic. */
export const same = (a, b) =>
  !!a && !!b && a.every((v, i) => Math.abs(v - b[i]) < 1e-6);
