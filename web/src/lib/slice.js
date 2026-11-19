/** Cutting a picture into strips: where the cuts go, and the strips made. */

/** The most strips a picture is cut into, whichever way it is asked for. */
export const MAX_SLICES = 50;

/**
 * Where the strips of a side `length` pixels long start and end, as
 * `[start, end)` pairs in order.
 *
 * `count` strips are as even as whole pixels allow, the odd pixels spread
 * among them rather than heaped on one. `size` pixels a strip gives every
 * strip that size, and the last one whatever is left over. Either way no
 * strip is empty, and there are never more than `MAX_SLICES`.
 */
export function cuts(length, { by, count = 2, size = 1080 }) {
  const total = Math.max(0, Math.floor(length));
  if (total < 1) return [];
  if (by === 'size') {
    const step = Math.max(1, Math.floor(size));
    // Far too small a size would make thousands of strips; the size is
    // raised to the one that makes the most allowed, rather than refused.
    const each = Math.max(step, Math.ceil(total / MAX_SLICES));
    const out = [];
    for (let start = 0; start < total; start += each) {
      out.push([start, Math.min(total, start + each)]);
    }
    return out;
  }
  const many = Math.min(Math.max(1, Math.floor(count)), total, MAX_SLICES);
  return Array.from({ length: many }, (_, i) =>
    [Math.round((i * total) / many), Math.round(((i + 1) * total) / many)]);
}

/** The strips of a `width` by `height` picture, as `{ x, y, width, height }`:
 *  `columns` side by side, or rows one above the other. */
export function strips(width, height, { across, ...how }) {
  if (across === 'rows') {
    return cuts(height, how).map(([a, b]) => ({ x: 0, y: a, width, height: b - a }));
  }
  return cuts(width, how).map(([a, b]) => ({ x: a, y: 0, width: b - a, height }));
}

/** A strip's file name: the picture's own, numbered so the files sort in the
 *  order the strips go, left to right or top to bottom. */
export function stripName(name, index, total, ext) {
  const stem = (name || 'photo').replace(/\.[^.]+$/, '');
  const digits = String(total).length;
  return `${stem}_${String(index + 1).padStart(digits, '0')}-of-${total}.${ext}`;
}

/** The type and extension to write strips in: the picture's own where the
 *  browser can write it back, JPEG otherwise. */
export function outputFor(type, { webp = false } = {}) {
  if (type === 'image/png') return { type, ext: 'png' };
  if (type === 'image/webp' && webp) return { type, ext: 'webp' };
  return { type: 'image/jpeg', ext: 'jpg' };
}

/** A picture handed from the developer to the slicing page, which takes it
 *  once: `{ blob, name, captured }`. Kept in the module, not in storage, so
 *  it lasts exactly as long as the visit. */
let handed = null;

export function handOff(picture) {
  handed = picture;
}

export function takeHandOff() {
  const picture = handed;
  handed = null;
  return picture;
}
