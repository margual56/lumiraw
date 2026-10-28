/** Ranking a scored gallery: bursts grouped, the best of each picked. Pure
 *  functions over plain objects, so `tools/check-cull.mjs` can run them. */

/** A share of blown or black pixels worth pointing out. A few specular
 *  highlights or a deep shadow stay under it; a burnt sky does not. */
export const CLIPPED = 0.01;

/** Every lever, as the page starts. */
export const DEFAULTS = Object.freeze({
  /** How much of the mark is the aesthetic network's, the rest technical. */
  weight: 0.6,
  /** Within a burst, how much the sharpest frame is favoured. */
  focus: 0.5,
  /** How much blown and crushed areas cost. */
  faults: 0.5,
  /** How much shut eyes cost. */
  eyes: 0.5,
  /** How much the picks are kept from looking like each other. */
  variety: 0.3,
  /** Seconds between frames that can still be one burst. */
  gap: 4,
  /** How alike two frames must look to be one burst. */
  similarity: 0.8,
});

/** One mark from the two networks', `weight` of it the aesthetic one. */
export const blend = (item, weight) => weight * item.aesthetic + (1 - weight) * item.technical;

/**
 * Marks lost to exposure faults, at full strength: a blown area costs more
 * than a black one, since a night sky or a silhouette is often meant and a
 * burnt sky seldom is. Small amounts are free, and no photo loses more than
 * three marks.
 */
export function exposurePenalty(item, faults) {
  const blown = Math.max(0, (item.blown ?? 0) - 0.005) * 20;
  const black = Math.max(0, (item.crushed ?? 0) - 0.05) * 4;
  return faults * Math.min(3, blown + black);
}

/**
 * How shut a face's eyes are, 0 open to 1 shut, from how open the landmarks
 * put each (height over width: about 0.25 open, under 0.08 shut, a smile's
 * squint between), or null when the face was too small to tell.
 */
export function shut(face) {
  if (!face?.eyes) return null;
  const open = (face.eyes[0] + face.eyes[1]) / 2;
  return Math.min(1, Math.max(0, (0.15 - open) / (0.15 - 0.08)));
}

/** The faces whose eyes count as shut. */
export const blinking = (item) => (item.faces ?? []).filter((f) => (shut(f) ?? 0) >= 0.5).length;

/**
 * Marks lost to shut eyes, at full strength: three for a portrait with its
 * eyes shut. In a group it is half the worst face and half the share of
 * faces shut, so one blink costs more than nothing and a row of them more.
 */
export function eyesPenalty(item, eyes) {
  const judged = (item.faces ?? []).map(shut).filter((v) => v != null);
  if (!judged.length || eyes <= 0) return 0;
  const worst = Math.max(...judged);
  const mean = judged.reduce((a, b) => a + b, 0) / judged.length;
  return eyes * 3 * (0.5 * worst + 0.5 * mean);
}

/** The sharpness of a photo's largest face, if one was measured. */
export const faceSharpness = (item) => (item.faces ?? []).find((f) => f.sharpness > 0)?.sharpness ?? 0;

/**
 * Within a burst, marks lost for being softer than its sharpest frame. The
 * Laplacian's variance goes with the square of the detail, so it is the
 * ratio's logarithm that counts: a quarter of the sharpest's variance (half
 * its detail) costs three marks at full strength, never more.
 */
export function focusPenalty(sharpness, sharpest, focus) {
  if (!(sharpest > 0) || focus <= 0) return 0;
  const ratio = Math.max(sharpness ?? 0, 1e-9) / sharpest;
  return focus * Math.min(3, -1.5 * Math.log2(Math.min(1, ratio)));
}

/**
 * How alike two photos look: the cosine between their features once the
 * gallery's mean is taken away. The raw features are all positive and every
 * pair of photographs looks alike in them; what one frame has that the rest
 * of the gallery does not is what tells a burst apart.
 *
 * Expanding the cosine leaves the dot product of the raw features as the
 * only part that needs both photos, and that does not change as the gallery
 * grows, so it is worked out once per pair and kept in `cache` (a WeakMap,
 * shared between calls). Everything else is one pass over the gallery.
 */
export function likeness(items, cache = new WeakMap()) {
  const scored = items.filter((it) => it.feature);
  if (!scored.length) return () => 0;
  const len = scored[0].feature.length;
  const mean = new Float64Array(len);
  for (const it of scored) {
    const f = it.feature;
    for (let i = 0; i < len; i += 1) mean[i] += f[i];
  }
  for (let i = 0; i < len; i += 1) mean[i] /= scored.length;
  let mm = 0;
  for (let i = 0; i < len; i += 1) mm += mean[i] * mean[i];

  const raw = (a, b) => {
    let row = cache.get(a);
    if (row?.has(b)) return row.get(b);
    const other = cache.get(b);
    if (other?.has(a)) return other.get(a);
    let s = 0;
    const fa = a.feature, fb = b.feature;
    for (let i = 0; i < len; i += 1) s += fa[i] * fb[i];
    if (!row) cache.set(a, (row = new Map()));
    row.set(b, s);
    return s;
  };
  const towardMean = new Map();
  for (const it of scored) {
    let s = 0;
    for (let i = 0; i < len; i += 1) s += it.feature[i] * mean[i];
    towardMean.set(it, s);
  }
  const norm = (it) => Math.sqrt(Math.max(1e-12, raw(it, it) - 2 * towardMean.get(it) + mm));
  return (a, b) => {
    if (!towardMean.has(a) || !towardMean.has(b)) return 0;
    return (raw(a, b) - towardMean.get(a) - towardMean.get(b) + mm) / (norm(a) * norm(b));
  };
}

/**
 * Each item's sharpness as a share of the gallery's sharpest, 0..100. The
 * Laplacian's variance has no scale of its own, so it is only told against
 * the other photographs. In a gallery of twenty or more the 95th percentile
 * stands in for the top, so that one freak does not flatten the rest; in a
 * smaller one there is no telling a freak from the sharpest.
 */
export function relativeSharpness(items) {
  const values = items.map((it) => it.sharpness).filter((v) => v > 0).sort((a, b) => a - b);
  const at = values.length >= 20 ? Math.floor((values.length - 1) * 0.95) : values.length - 1;
  const top = values[at] || 1;
  return new Map(items.map((it) => [it, Math.min(100, (100 * (it.sharpness || 0)) / top)]));
}

/** Shooting order: by when, then by name as a camera numbers its files. */
const byShot = (a, b) => (a.captured || 0) - (b.captured || 0)
  || a.name.localeCompare(b.name, undefined, { numeric: true });

/** Photos without a time are all compared with each other up to this many;
 *  past it, each only with its neighbours in name order. */
const ALL_PAIRS = 600;
const NEIGHBOURS = 40;

/**
 * Bursts, in shooting order. Two frames are one burst when they were taken
 * within `gap` seconds of each other and look alike, and a burst is every
 * frame joined to it that way, so a duplicate does not have to sit next to
 * its twin. Photos that do not say when they were taken are matched on
 * likeness alone. Each group is an array of items; a lone photo is a group
 * of one.
 */
export function groupBursts(items, { gap = DEFAULTS.gap, similarity = DEFAULTS.similarity,
                                     like = likeness(items) } = {}) {
  const sorted = items.filter((it) => it.feature).sort(byShot);
  const parent = sorted.map((_, i) => i);
  const find = (i) => {
    while (parent[i] !== i) i = parent[i] = parent[parent[i]];
    return i;
  };
  const join = (i, j) => {
    if (like(sorted[i], sorted[j]) < similarity) return;
    const [a, b] = [find(i), find(j)];
    if (a !== b) parent[Math.max(a, b)] = Math.min(a, b);
  };
  const untimed = [];
  const timed = [];
  sorted.forEach((it, i) => (it.captured ? timed : untimed).push(i));
  // Sorted by time, so each frame only looks forward until the gap is passed.
  for (let k = 0; k < timed.length; k += 1) {
    for (let l = k + 1; l < timed.length; l += 1) {
      if (sorted[timed[l]].captured - sorted[timed[k]].captured > gap) break;
      join(timed[k], timed[l]);
    }
  }
  const reach = untimed.length <= ALL_PAIRS ? untimed.length : NEIGHBOURS;
  for (let k = 0; k < untimed.length; k += 1) {
    for (let l = k + 1; l < Math.min(untimed.length, k + 1 + reach); l += 1) join(untimed[k], untimed[l]);
  }
  const groups = new Map();
  sorted.forEach((it, i) => {
    const root = find(i);
    if (!groups.has(root)) groups.set(root, []);
    groups.get(root).push(it);
  });
  return [...groups.values()];
}

/** How many photographs to keep: a count, or a share of all of them. */
export function keepCount(total, { mode, value }) {
  if (mode === 'percent') return Math.max(1, Math.ceil(total * value / 100));
  return Math.max(0, Math.round(value));
}

/**
 * The picks. Within each group the frames are ranked by their mark less
 * what their exposure and (against the group's sharpest) their focus cost;
 * the first of each leads it. Then the leaders are taken one at a time, best
 * first, each one's mark lowered by how much it looks like a pick already
 * made, up to two marks at `variety` 1, so that five views of one scene do
 * not crowd out everything else.
 *
 * Within a burst whose every frame shows a face, focus is judged on the
 * largest face rather than the whole frame.
 *
 * Returns the picked set; every group with its best frame first, picks first
 * in the order they were chosen and the rest best first; and each item's
 * marks: `score` (the mark less exposure faults and shut eyes), `eyes` (what
 * shut eyes cost), `focus` (what softness cost it in its group) and `rank`
 * (what it was ranked by in its group).
 */
export function pick(groups, given, keep, total, like = () => 0) {
  const levers = { ...DEFAULTS, ...given };
  const marks = new Map();
  const ranked = groups.map((group) => {
    // Where every frame has a face, it is the face that has to be in focus.
    const onFaces = group.length > 1 && group.every((it) => faceSharpness(it) > 0);
    const sharp = onFaces ? faceSharpness : (it) => it.sharpness ?? 0;
    const sharpest = group.length > 1 ? Math.max(...group.map(sharp)) : 0;
    for (const it of group) {
      const eyes = eyesPenalty(it, levers.eyes);
      const score = blend(it, levers.weight) - exposurePenalty(it, levers.faults) - eyes;
      const focus = group.length > 1 ? focusPenalty(sharp(it), sharpest, levers.focus) : 0;
      marks.set(it, { score, eyes, focus, rank: score - focus });
    }
    return [...group].sort((a, b) => marks.get(b).rank - marks.get(a).rank);
  });
  const lead = (group) => marks.get(group[0]).score;
  const pool = [...ranked].sort((a, b) => lead(b) - lead(a));
  const count = Math.min(pool.length, keepCount(total, keep));

  const chosen = [];
  if (levers.variety > 0) {
    // How like a pick each remaining leader is, updated as picks are made.
    const closest = new Map(pool.map((g) => [g, 0]));
    while (chosen.length < count) {
      let best = null;
      let bestValue = -Infinity;
      for (const g of pool) {
        if (!closest.has(g)) continue;
        const value = lead(g) - 2 * levers.variety * closest.get(g);
        if (value > bestValue) { best = g; bestValue = value; }
      }
      chosen.push(best);
      closest.delete(best);
      for (const [g, near] of closest) {
        closest.set(g, Math.max(near, like(best[0], g[0])));
      }
    }
  } else {
    chosen.push(...pool.slice(0, count));
  }
  const taken = new Set(chosen);
  return {
    picked: new Set(chosen.map((g) => g[0])),
    groups: [...chosen, ...pool.filter((g) => !taken.has(g))],
    marks,
  };
}

const field = (v) => (/[",\n]/.test(String(v)) ? `"${String(v).replace(/"/g, '""')}"` : String(v));

/** The ranking as CSV, one row per photograph, in the order the page shows
 *  them. Shares are percentages; sharpness is against the gallery's
 *  sharpest (100). */
export function toCsv({ groups, marks }, picked, levers, sharp = relativeSharpness(groups.flat())) {
  const lines = [['file', 'group', 'aesthetic', 'technical', 'blend', 'exposure_penalty',
                  'eyes_penalty', 'focus_penalty', 'score', 'blown_pct', 'black_pct',
                  'brightness_pct', 'sharpness', 'faces', 'eyes_shut', 'picked'].join(',')];
  const pct = (v) => (v == null ? '' : (v * 100).toFixed(2));
  groups.forEach((group, n) => {
    for (const it of group) {
      const m = marks.get(it);
      lines.push([field(it.name), n + 1, it.aesthetic.toFixed(3), it.technical.toFixed(3),
                  blend(it, levers.weight).toFixed(3), exposurePenalty(it, levers.faults).toFixed(3),
                  m.eyes.toFixed(3), m.focus.toFixed(3), m.score.toFixed(3), pct(it.blown),
                  pct(it.crushed), pct(it.brightness), (sharp.get(it) ?? 0).toFixed(1),
                  (it.faces ?? []).length, blinking(it), picked.has(it) ? 1 : 0].join(','));
    }
  });
  return `${lines.join('\n')}\n`;
}
