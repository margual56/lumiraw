/**
 * The named looks, as settings of the grade's own controls: the curves, the
 * wheels, the mixer and monochrome. Choosing one fills those controls in, and
 * from there they are the photographer's to move; nothing else renders a look.
 *
 * `looks.json` is written by `core/src/bin/fitlooks.rs`, which fits the
 * controls to each look's source table (see `core/src/looks.rs`).
 */
import PRESETS from './looks.json';

/** The controls a look sets, and nothing else. */
const KEYS = ['curves', 'wheels', 'mixer', 'monochrome'];

const blankCurves = () => ({ strength: 1, rgb: [], r: [], g: [], b: [] });
// As `state.svelte.js` has them, written out here so the two modules do not
// import each other.
const wheel = () => ({ hue: 0, amount: 0, lum: 0 });
const blankWheels = () => ({ shadows: wheel(), midtones: wheel(), highlights: wheel(),
                             global: wheel(), balance: 0, blending: 0.5 });

/** The controls look `id` puts in, fresh copies; 'none' (or '') clears them. */
export function lookControls(id) {
  const preset = PRESETS[id];
  if (!preset) return { curves: blankCurves(), wheels: blankWheels(), mixer: {}, monochrome: 0 };
  return structuredClone(Object.fromEntries(KEYS.map((k) => [k, preset[k]])));
}

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** Two wheels as one, their pushes added. */
function addWheel(a, b, k) {
  const push = (w, s) => {
    const r = (w.hue * Math.PI) / 180;
    return [Math.cos(r) * w.amount * s, Math.sin(r) * w.amount * s];
  };
  const [ax, ay] = push(a, 1);
  const [bx, by] = push(b, k);
  const x = ax + bx, y = ay + by;
  const amount = Math.min(1, Math.hypot(x, y));
  const hue = amount < 1e-4 ? a.hue : ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
  return { hue: Math.round(hue * 10) / 10, amount: Math.round(amount * 1000) / 1000,
           lum: Math.round(clamp(a.lum + b.lum * k, -1, 1) * 1000) / 1000 };
}

const curvesUntouched = (c) => !c || (['rgb', 'r', 'g', 'b'].every((k) => !c[k]?.length)
  && Object.values(c.regions ?? {}).every((v) => !v));

/**
 * Settings saved while a look was a table laid under the controls (they say
 * how much of it with `look_strength`), with that look folded into the
 * controls instead, as near as they come: its wheels and mixer added to the
 * photographer's, its curves used where there were none of their own, faded
 * by the strength it had. Anything else comes back as it was.
 */
export function foldTableLook(saved) {
  if (!saved || !('look_strength' in saved)) return saved;
  const { look_strength: strength, ...rest } = saved;
  const preset = PRESETS[rest.look];
  if (!preset) return { ...rest, look: '' };
  const k = clamp(Number(strength ?? 1), 0, 1);
  const own = rest.wheels ?? blankWheels();
  const wheels = { ...own };
  for (const z of ['shadows', 'midtones', 'highlights', 'global']) {
    wheels[z] = addWheel(own[z], preset.wheels[z], k);
  }
  const mixer = { ...(rest.mixer ?? {}) };
  for (const [band, v] of Object.entries(preset.mixer)) {
    const mine = mixer[band] ?? { h: 0, s: 0, l: 0 };
    mixer[band] = Object.fromEntries(['h', 's', 'l'].map((c) =>
      [c, Math.round(clamp((mine[c] ?? 0) + v[c] * k, -1, 1) * 100) / 100]));
  }
  // Two sets of curve points do not add up to one; the photographer's own
  // win, and the look's are used only where there were none.
  const curves = curvesUntouched(rest.curves)
    ? { ...structuredClone(preset.curves), strength: k } : rest.curves;
  const monochrome = Math.max(rest.monochrome ?? 0, preset.monochrome * k);
  return { ...rest, wheels, mixer, curves, monochrome };
}
