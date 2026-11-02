/** The night sky over a place: the moon, the bright planets, and how dark
 *  the sky is and so how faint a star the eye can make out. Pure, so
 *  `tools/check-calc.mjs` can run it; the stars themselves are in
 *  sky-data.js, loaded with the calculator that draws them. */

import { RAD, atMinute, localDay, wrap360 } from './geo.js';
import { skyFrom, sunCoords, sunPosition } from './calc.js';

const J2000 = 2451545;
const julian = (date) => date.valueOf() / 864e5 + 2440587.5;
const OBLIQUITY = 23.43928 * RAD;

/** From the ecliptic (radians) to right ascension and declination (degrees). */
function equatorial(l, b) {
  const ra = Math.atan2(Math.sin(l) * Math.cos(OBLIQUITY) - Math.tan(b) * Math.sin(OBLIQUITY), Math.cos(l));
  const dec = Math.asin(Math.sin(b) * Math.cos(OBLIQUITY) + Math.cos(b) * Math.sin(OBLIQUITY) * Math.sin(l));
  return { ra: wrap360(ra / RAD), dec: dec / RAD };
}

// -- the moon -----------------------------------------------------------------------
// After SunCalc (BSD), itself after https://aa.quae.nl/en/reken/hemelpositie.html:
// good to a fraction of a degree.

/** The moon's right ascension and declination (degrees) and distance (km) at `date`. */
export function moonCoords(date) {
  const d = julian(date) - J2000;
  const L = RAD * (218.316 + 13.176396 * d);
  const M = RAD * (134.963 + 13.064993 * d);
  const F = RAD * (93.272 + 13.22935 * d);
  return { ...equatorial(L + RAD * 6.289 * Math.sin(M), RAD * 5.128 * Math.sin(F)), km: 385001 - 20905 * Math.cos(M) };
}

/**
 * Where the moon stands from a place, through `look` (calc.js's `skyFrom`
 * for the place and `date`): lower than from the earth's centre by its
 * parallax, nearly a degree at the horizon.
 */
export function moonFrom(look, date) {
  const m = moonCoords(date);
  const p = look(m.ra, m.dec);
  p.altitude -= Math.asin(6378 / m.km) / RAD * Math.cos(p.altitude * RAD);
  return p;
}

/** How much of the moon is lit at `date` (0 new to 1 full), and whether it is waxing. */
export function moonPhase(date) {
  const sun = sunCoords(date);
  const m = moonCoords(date);
  const [sa, sd, ma, md] = [sun.ra, sun.dec, m.ra, m.dec].map((x) => x * RAD);
  const elongation = Math.acos(Math.sin(sd) * Math.sin(md) + Math.cos(sd) * Math.cos(md) * Math.cos(sa - ma));
  const incidence = Math.atan2(149598000 * Math.sin(elongation), m.km - 149598000 * Math.cos(elongation));
  // Which way the lit side faces, from north through east: east of north is the sun rising after it, waning.
  const towards = Math.atan2(Math.cos(sd) * Math.sin(sa - ma), Math.sin(sd) * Math.cos(md) - Math.cos(sd) * Math.sin(md) * Math.cos(sa - ma));
  return { lit: (1 + Math.cos(incidence)) / 2, waxing: towards < 0 };
}

// -- the planets --------------------------------------------------------------------
// The approximate orbits JPL gives for 1800–2050 (E. M. Standish,
// https://ssd.jpl.nasa.gov/planets/approx_pos.html): to a minute of arc or so
// for these, as each changes per century from J2000. Each row is the
// semi-major axis (au), eccentricity, inclination, mean longitude, longitude
// of perihelion and of the ascending node (degrees), and then their rates.

const ORBITS = {
  mercury: [0.38709927, 0.20563593, 7.00497902, 252.2503235, 77.45779628, 48.33076593,
    0.00000037, 0.00001906, -0.00594749, 149472.67411175, 0.16047689, -0.12534081],
  venus: [0.72333566, 0.00677672, 3.39467605, 181.9790995, 131.60246718, 76.67984255,
    0.0000039, -0.00004107, -0.0007889, 58517.81538729, 0.00268329, -0.27769418],
  earth: [1.00000261, 0.01671123, -0.00001531, 100.46457166, 102.93768193, 0,
    0.00000562, -0.00004392, -0.01294668, 35999.37244981, 0.32327364, 0],
  mars: [1.52371034, 0.0933941, 1.84969142, -4.55343205, -23.94362959, 49.55953891,
    0.00001847, 0.00007882, -0.00813131, 19140.30268499, 0.44441088, -0.29257343],
  jupiter: [5.202887, 0.04838624, 1.30439695, 34.39644051, 14.72847983, 100.47390909,
    -0.00011607, -0.00013253, -0.00183714, 3034.74612775, 0.21252668, 0.20469106],
  saturn: [9.53667594, 0.05386179, 2.48599187, 49.95424423, 92.59887831, 113.66242448,
    -0.0012506, -0.00050991, 0.00193609, 1222.49362201, -0.41897216, -0.28867794],
};
/** Each planet's brightness at 1 au from the sun and the earth, by its phase
 *  angle in degrees: a polynomial's coefficients, from Mallama and Hilton's
 *  fits (2018). Saturn's rings left out, so it is off by up to half a magnitude. */
const BRIGHTNESS = {
  mercury: [-0.613, 6.328e-2, -1.6336e-3, 3.3644e-5, -3.4265e-7, 1.6893e-9, -3.0334e-12],
  venus: [-4.384, -1.044e-3, 3.687e-4, -2.814e-6, 8.938e-9],
  mars: [-1.601, 2.267e-2, -1.302e-4],
  jupiter: [-9.395, -3.7e-4, 6.16e-4],
  saturn: [-8.88, 4.4e-2],
};
export const PLANETS = Object.keys(BRIGHTNESS);

/** Where a planet is about the sun, in au, on the ecliptic of J2000. */
function heliocentric(name, centuries) {
  const o = ORBITS[name];
  const [a, e, i, L, perihelion, node] = o.slice(0, 6).map((x, k) => x + o[k + 6] * centuries);
  const w = (perihelion - node) * RAD;
  const M = (wrap360(L - perihelion + 180) - 180) * RAD;
  // Kepler's equation, a few rounds of Newton's.
  let E = M + e * Math.sin(M);
  for (let k = 0; k < 6; k++) E -= (E - e * Math.sin(E) - M) / (1 - e * Math.cos(E));
  const x = a * (Math.cos(E) - e);
  const y = a * Math.sqrt(1 - e * e) * Math.sin(E);
  const [cw, sw, cn, sn, ci, si] = [Math.cos(w), Math.sin(w), Math.cos(node * RAD), Math.sin(node * RAD), Math.cos(i * RAD), Math.sin(i * RAD)];
  return [
    (cw * cn - sw * sn * ci) * x + (-sw * cn - cw * sn * ci) * y,
    (cw * sn + sw * cn * ci) * x + (-sw * sn + cw * cn * ci) * y,
    sw * si * x + cw * si * y,
  ];
}

/** The bright planets at `date`: name, right ascension and declination (degrees), and magnitude. */
export function planets(date) {
  const centuries = (julian(date) - J2000) / 36525;
  const earth = heliocentric('earth', centuries);
  return PLANETS.map((name) => {
    const p = heliocentric(name, centuries);
    const g = p.map((x, k) => x - earth[k]);
    const r = Math.hypot(...p);
    const distance = Math.hypot(...g);
    const phase = Math.acos((r * r + distance * distance - Math.hypot(...earth) ** 2) / (2 * r * distance)) / RAD;
    return {
      name,
      ...equatorial(Math.atan2(g[1], g[0]), Math.atan2(g[2], Math.hypot(g[0], g[1]))),
      mag: BRIGHTNESS[name].reduce((sum, c, k) => sum + c * phase ** k, 0) + 5 * Math.log10(r * distance),
    };
  });
}

// -- how dark ----------------------------------------------------------------------

/** The sky's brightness over a moonless, unlit sky, by the sun's altitude
 *  (degrees, before the air's lift): magnitudes per square arcsecond brighter. */
const TWILIGHT = [[-18, 0], [-15, 1.2], [-12, 4], [-9, 7], [-6, 10], [-3, 12.5], [0, 14]];

/**
 * How bright the sky is, in magnitudes per square arcsecond (darker is
 * higher; 22 is a natural sky), from the light on the ground (`ratio`, the
 * atlas's artificial brightness over the natural sky, darksky.js), the sun's
 * altitude and the moon's (degrees) and how much of it is lit. The moon's is
 * a rough sum: a full moon high up brings a dark sky to about 19.
 */
export function skyBrightness({ ratio = 0, sun, moon, lit }) {
  const i = TWILIGHT.findIndex(([h]) => h > sun);
  let brighter;
  if (i === 0) brighter = 0;
  else if (i < 0) brighter = TWILIGHT.at(-1)[1] + (sun - TWILIGHT.at(-1)[0]) * 0.5;
  else {
    const [h0, b0] = TWILIGHT[i - 1];
    const [h1, b1] = TWILIGHT[i];
    brighter = b0 + ((b1 - b0) * (sun - h0)) / (h1 - h0);
  }
  // The lit fraction cubed: a half moon gives a tenth of a full one's light.
  const moonlight = moon > 0 ? 15 * lit ** 3 * Math.sqrt(Math.sin(moon * RAD)) : 0;
  return 22 - 2.5 * Math.log10(10 ** (brighter / 2.5) + ratio + moonlight);
}

/** The faintest star the eye can see in a sky that bright (Schaefer's
 *  relation, as amateur astronomers use it): 6.6 in a natural one, 4 in a town. */
export const faintest = (sqm) => 7.93 - 5 * Math.log10(10 ** (4.316 - sqm / 5) + 1);

// -- the whole sky ------------------------------------------------------------------

/** The centre of the galaxy, the Milky Way's brightest part, in Sagittarius. */
export const CORE = { ra: 266.4, dec: -28.94 };

/** The Milky Way's cells that are on it, as right ascension, declination and level (sky-data's grid). */
const cellsOf = new WeakMap();
function milkyCells(data) {
  if (!cellsOf.has(data)) {
    const across = 360 / data.MILKY_GRID;
    const cells = [];
    for (let i = 0; i < data.MILKY.length; i++) {
      const level = data.MILKY.charCodeAt(i) - 48;
      if (!level) continue;
      cells.push({
        ra: ((i % across) + 0.5) * data.MILKY_GRID,
        dec: -90 + (Math.floor(i / across) + 0.5) * data.MILKY_GRID,
        level,
      });
    }
    cellsOf.set(data, cells);
  }
  return cellsOf.get(data);
}

/**
 * The night sky at `date` over a place, everything where it stands
 * (`bearing` and `altitude`, degrees): `stars` four numbers each (bearing,
 * altitude, magnitude, B−V), the constellations' `lines` as runs of bearing
 * and altitude and their `labels`, the bright stars' `names`, the Milky
 * Way's `milky` cells (`milkyGrid` degrees apart), the `planets`, the `moon` (with how much is lit) and
 * the galaxy's `core`. Only what is above the horizon, or near it, for the
 * stars and cells. `data` is sky-data.js; `lang` picks the names.
 */
export function nightSky(data, date, lat, lon, lang) {
  const look = skyFrom(date, lat, lon);
  const es = lang === 'es';
  const stars = [];
  for (let i = 0; i < data.STARS.length; i += 4) {
    const p = look(data.STARS[i], data.STARS[i + 1]);
    if (p.altitude > -2) stars.push(p.bearing, p.altitude, data.STARS[i + 2], data.STARS[i + 3]);
  }
  const names = data.NAMES.map(([i, en, spanish]) => ({
    name: es ? spanish : en, mag: data.STARS[i * 4 + 2], ...look(data.STARS[i * 4], data.STARS[i * 4 + 1]),
  }));
  const lines = [];
  const labels = [];
  for (const c of data.CONSTELLATIONS) {
    for (const run of c.lines) {
      const out = [];
      for (let k = 0; k < run.length; k += 2) {
        const p = look(run[k], run[k + 1]);
        out.push(p.bearing, p.altitude);
      }
      lines.push(out);
    }
    labels.push({ name: es ? c.es : c.en, rank: c.rank, ...look(...c.at) });
  }
  const milky = [];
  for (const cell of milkyCells(data)) {
    const p = look(cell.ra, cell.dec);
    if (p.altitude > -5) milky.push({ ...p, level: cell.level, dec: cell.dec });
  }
  return {
    stars,
    names,
    lines,
    labels,
    milky,
    milkyGrid: data.MILKY_GRID,
    planets: planets(date).map((p) => ({ name: p.name, mag: p.mag, ...look(p.ra, p.dec) })),
    moon: { ...moonFrom(look, date), ...moonPhase(date) },
    core: look(CORE.ra, CORE.dec),
  };
}

/** The sun's depth at which a night counts as fully dark, and the core's height at which it is worth shooting. */
export const FULL_DARK = -18;
const CORE_HIGH = 10;

/** How far into the night a sun this deep makes it: 0 by day, 1 once the stars are all out. */
export const nightness = (sun) => Math.max(0, Math.min(1, (-sun - 4) / 10));

/**
 * The night after `day` (`YYYY-MM-DD`) from a spot, noon to noon, every `step`
 * minutes: `{ m, date, sun, moon, core }`, minutes from midnight on `day`, how
 * far down the sun is, and where the moon and the Milky Way's core are. All
 * that is said about a night is read from this, so it all agrees.
 */
export function nightOf({ lat, lon, day, step = 5 }) {
  const samples = [];
  for (let m = 720; m <= 2160; m += step) {
    const date = atMinute(day, m);
    const look = skyFrom(date, lat, lon);
    const { ra, dec } = sunCoords(date);
    samples.push({ m, date, sun: look(ra, dec).geometric, moon: moonFrom(look, date), core: look(CORE.ra, CORE.dec) });
  }
  return samples;
}

/** When a night is fully dark, `{ from, until }` in its minutes, or null. */
export function darkSpan(samples) {
  const black = samples.filter((p) => p.sun < FULL_DARK);
  return black.length ? { from: black[0].m, until: black.at(-1).m } : null;
}

/** The core at its highest while it is dark and `clear(p)` of the land, or null. */
export function coreBest(samples, clear = (p) => p.altitude > 0) {
  let best = null;
  for (const { m, sun, core } of samples) {
    if (sun < FULL_DARK && clear(core) && (!best || core.altitude > best.altitude)) best = { m, ...core };
  }
  return best;
}

/** The moon's night: when it rises and sets, by its top edge, whether it starts up, and its phase in the middle of the dark. */
export function moonOfNight(samples) {
  const up = (p) => p.moon.altitude + 0.27 > 0;
  let rise = null;
  let set = null;
  for (let i = 1; i < samples.length; i++) {
    if (up(samples[i]) && !up(samples[i - 1])) rise ??= samples[i].m;
    if (up(samples[i - 1]) && !up(samples[i])) set ??= samples[i].m;
  }
  const dark = darkSpan(samples);
  const middle = dark ? (dark.from + dark.until) / 2 : 1440;
  return { rise, set, up: up(samples[0]), ...moonPhase(samples.find((p) => p.m >= middle).date) };
}

/**
 * The nights after `count` days from `first` (`YYYY-MM-DD`), each
 * `{ day, from, moonless, core, cloud, score }`: when it gets fully dark, in
 * minutes from midnight on `day`; the hours of dark without the moon; the
 * Milky Way's core at its highest then; the mean cloud cover, where
 * `cloudAt(date)` knows it. `clear(p)` says whether a point stands clear of the land.
 */
export function bestNights({ lat, lon, first, count = 30, clear = (p) => p.altitude > 0, cloudAt = () => null }) {
  const nights = [];
  for (let i = 0; i < count; i++) {
    const start = new Date(`${first}T00:00`);
    start.setDate(start.getDate() + i);
    const day = localDay(start);
    const samples = nightOf({ lat, lon, day });
    const hours = (samples[1].m - samples[0].m) / 60;
    const { lit } = moonOfNight(samples);
    let moonless = 0;
    let coreHours = 0;
    const covers = [];
    for (const { date, sun, moon, core } of samples) {
      if (sun >= FULL_DARK) continue;
      const moonFree = moon.altitude + 0.27 <= 0 || lit < 0.1;
      if (moonFree) moonless += hours;
      if (moonFree && clear(core) && core.altitude >= CORE_HIGH) coreHours += hours;
      const cover = cloudAt(date);
      if (cover != null) covers.push(cover);
    }
    const cloud = covers.length ? covers.reduce((a, b) => a + b) / covers.length : null;
    nights.push({
      day, from: darkSpan(samples)?.from ?? null, moonless, core: coreBest(samples, clear), cloud,
      score: (moonless + 0.5 * coreHours) * (1 - (cloud ?? 0) / 100),
    });
  }
  return nights;
}

/** The best of `nights` that have any dark at all, best first. */
export const topNights = (nights, count = 3) =>
  nights.filter((night) => night.score > 0).toSorted((a, b) => b.score - a.score).slice(0, count);

/**
 * How the sky looks from a spot on `day` at `minutes` from midnight, or, before
 * the stars are all out then, at the darkest moment of the night after it:
 * `{ minutes, glow, limit }`, its brightness (mag/arcsec²) and the faintest star seen.
 * `ratio` is the atlas's light pollution there.
 */
export function skyOfNight({ lat, lon, day, minutes, ratio = 0 }) {
  const date = atMinute(day, minutes);
  let at = { m: minutes, date, sun: sunPosition(date, lat, lon).geometric };
  if (nightness(at.sun) < 1) at = nightOf({ lat, lon, day }).reduce((a, b) => (b.sun < a.sun ? b : a));
  const glow = skyBrightness({
    ratio, sun: at.sun, moon: moonFrom(skyFrom(at.date, lat, lon), at.date).altitude, lit: moonPhase(at.date).lit,
  });
  return { minutes: at.m, glow, limit: faintest(glow) };
}
