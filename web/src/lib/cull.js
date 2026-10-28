/** Ranking a scored gallery: bursts grouped, the best of each picked. Pure
 *  functions over plain objects, so `tools/check-cull.mjs` can run them. */

/** A share of blown or black pixels worth pointing out. A few specular
 *  highlights or a deep shadow stay under it; a burnt sky does not. */
export const CLIPPED = 0.01;

/** One mark from the two networks', `weight` of it the aesthetic one. */
export const blend = (item, weight) => weight * item.aesthetic + (1 - weight) * item.technical;

/**
 * Every item's features with the gallery's mean taken away, at unit length.
 * The raw features are all positive and every pair of photographs looks
 * alike in them; what one frame has that the rest of the gallery does not is
 * what tells a burst apart.
 */
export function centred(items) {
  const scored = items.filter((it) => it.feature);
  if (!scored.length) return new Map();
  const len = scored[0].feature.length;
  const mean = new Float32Array(len);
  for (const it of scored) for (let i = 0; i < len; i += 1) mean[i] += it.feature[i] / scored.length;
  const out = new Map();
  for (const it of scored) {
    const v = new Float32Array(len);
    let norm = 0;
    for (let i = 0; i < len; i += 1) { v[i] = it.feature[i] - mean[i]; norm += v[i] * v[i]; }
    norm = Math.sqrt(norm) || 1;
    for (let i = 0; i < len; i += 1) v[i] /= norm;
    out.set(it, v);
  }
  return out;
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

const dot = (a, b) => {
  let s = 0;
  for (let i = 0; i < a.length; i += 1) s += a[i] * b[i];
  return s;
};

/** Shooting order: by when, then by name as a camera numbers its files. */
const byShot = (a, b) => (a.captured || 0) - (b.captured || 0)
  || a.name.localeCompare(b.name, undefined, { numeric: true });

/**
 * Bursts, in shooting order: a frame joins the one before it when it was
 * taken within `gap` seconds (when both say) and looks like it. Each group is
 * an array of items; a lone photograph is a group of one.
 */
export function groupBursts(items, { gap = 4, similarity = 0.8, units = centred(items) } = {}) {
  const groups = [];
  let last = null;
  for (const it of [...items].filter((x) => units.has(x)).sort(byShot)) {
    const timed = it.captured && last?.captured;
    const close = !timed || Math.abs(it.captured - last.captured) <= gap;
    if (last && close && dot(units.get(it), units.get(last)) >= similarity) {
      groups[groups.length - 1].push(it);
    } else {
      groups.push([it]);
    }
    last = it;
  }
  return groups;
}

/** How many photographs to keep: a count, or a share of all of them. */
export function keepCount(total, { mode, value }) {
  if (mode === 'percent') return Math.max(1, Math.ceil(total * value / 100));
  return Math.max(0, Math.round(value));
}

/**
 * The picks: the best frame of each group, then the best of those. Returns
 * the set of picked items and each group's best frame first.
 */
export function pick(groups, weight, keep, total) {
  const ranked = groups.map((g) => [...g].sort((a, b) => blend(b, weight) - blend(a, weight)));
  const leaders = ranked.map((g) => g[0]).sort((a, b) => blend(b, weight) - blend(a, weight));
  return { picked: new Set(leaders.slice(0, keepCount(total, keep))), groups: ranked };
}

const field = (v) => (/[",\n]/.test(String(v)) ? `"${String(v).replace(/"/g, '""')}"` : String(v));

/** The ranking as CSV, one row per photograph, best first. Shares are
 *  percentages; sharpness is against the gallery's sharpest (100). */
export function toCsv(groups, picked, weight, sharp = relativeSharpness(groups.flat())) {
  const rows = groups.flatMap((g, n) => g.map((it) => ({ it, group: n + 1 })))
    .sort((a, b) => blend(b.it, weight) - blend(a.it, weight));
  const lines = [['file', 'group', 'aesthetic', 'technical', 'score', 'blown_pct', 'black_pct',
                  'brightness_pct', 'sharpness', 'picked'].join(',')];
  const pct = (v) => (v == null ? '' : (v * 100).toFixed(2));
  for (const { it, group } of rows) {
    lines.push([field(it.name), group, it.aesthetic.toFixed(3), it.technical.toFixed(3),
                blend(it, weight).toFixed(3), pct(it.blown), pct(it.crushed), pct(it.brightness),
                (sharp.get(it) ?? 0).toFixed(1), picked.has(it) ? 1 : 0].join(','));
  }
  return `${lines.join('\n')}\n`;
}
