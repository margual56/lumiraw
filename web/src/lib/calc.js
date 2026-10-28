/** The photographer's arithmetic: depth of field, ND filters, sharp stars and
 *  the light at either end of the day. Pure functions over plain numbers, so
 *  `tools/check-calc.mjs` can run them. Lengths in millimetres unless named
 *  otherwise, distances in metres, times in seconds. */

/** Common sensors by crop factor, against 35 mm film (36 × 24 mm). */
export const SENSORS = [
  { id: 'medium', crop: 0.79 },
  { id: 'full', crop: 1 },
  { id: 'apsc', crop: 1.5 },
  { id: 'canon', crop: 1.6 },
  { id: 'mft', crop: 2 },
  { id: 'inch', crop: 2.7 },
];

/** The circle of confusion: the largest blur that still looks sharp in a
 *  print seen whole. 0.03 mm on full frame, smaller on smaller sensors. */
export const confusion = (crop) => 0.03 / crop;

/**
 * Depth of field for a lens focused at `distance` metres: the near and far
 * limits of what looks sharp (far is Infinity past the hyperfocal distance),
 * and the hyperfocal distance itself, all in metres.
 */
export function depthOfField({ focal, aperture, distance, crop }) {
  const f = focal;
  const c = confusion(crop);
  const s = distance * 1000;
  const hyper = (f * f) / (aperture * c) + f;
  const near = (s * (hyper - f)) / (hyper + s - 2 * f);
  const far = s >= hyper ? Infinity : (s * (hyper - f)) / (hyper - s);
  return { near: near / 1000, far: far / 1000, total: (far - near) / 1000, hyperfocal: hyper / 1000 };
}

/** The shutter time with an ND filter of `stops` in front: each stop doubles it. */
export const ndShutter = (base, stops) => base * 2 ** stops;

/** Common ND filters, by their name on the box and the stops they take. */
export const FILTERS = [
  { name: 'ND2', stops: 1 }, { name: 'ND4', stops: 2 }, { name: 'ND8', stops: 3 },
  { name: 'ND16', stops: 4 }, { name: 'ND64', stops: 6 }, { name: 'ND1000', stops: 10 },
  { name: 'ND32000', stops: 15 }, { name: 'ND1000000', stops: 20 },
];

/** Shutter speeds a camera offers, in thirds of a stop as most dials step. */
export const SHUTTERS = [
  1 / 8000, 1 / 6400, 1 / 5000, 1 / 4000, 1 / 3200, 1 / 2500, 1 / 2000, 1 / 1600, 1 / 1250,
  1 / 1000, 1 / 800, 1 / 640, 1 / 500, 1 / 400, 1 / 320, 1 / 250, 1 / 200, 1 / 160, 1 / 125,
  1 / 100, 1 / 80, 1 / 60, 1 / 50, 1 / 40, 1 / 30, 1 / 25, 1 / 20, 1 / 15, 1 / 13, 1 / 10,
  1 / 8, 1 / 6, 1 / 5, 1 / 4, 0.3, 0.4, 0.5, 0.6, 0.8, 1, 1.3, 1.6, 2, 2.5, 3.2, 4, 5, 6, 8,
  10, 13, 15, 20, 25, 30,
];

/**
 * The longest exposure before stars trail, two ways. The 500 rule is the
 * old one, from film: 500 over the full-frame focal length. NPF (Frédéric
 * Michaud's, in its simple form) also counts the aperture and the size of a
 * pixel, which the 500 rule predates, and is the one to trust on a modern
 * sensor at 100 % view.
 */
export function starRules({ focal, crop, aperture, megapixels, aspect = 3 / 2 }) {
  const width = 36 / crop;
  const pixels = Math.sqrt(megapixels * 1e6 * aspect);
  const pitch = (width / pixels) * 1000;
  return { rule500: 500 / (focal * crop), npf: (35 * aperture + 30 * pitch) / focal, pitch };
}

// --- the sun ---------------------------------------------------------------------
// After Vladimir Agafonkin's SunCalc (BSD), itself after the formulas at
// https://aa.quae.nl/en/reken/zonpositie.html: good to a couple of minutes, which is
// as well as the light can be planned anyway.

const RAD = Math.PI / 180;
const DAY_MS = 864e5;
const J1970 = 2440588;
const J2000 = 2451545;
const OBLIQUITY = RAD * 23.4397;
const J0 = 0.0009;

const toDays = (date) => date.valueOf() / DAY_MS - 0.5 + J1970 - J2000;
const fromJulian = (j) => new Date((j + 0.5 - J1970) * DAY_MS);
const meanAnomaly = (d) => RAD * (357.5291 + 0.98560028 * d);
function eclipticLongitude(m) {
  const centre = RAD * (1.9148 * Math.sin(m) + 0.02 * Math.sin(2 * m) + 0.0003 * Math.sin(3 * m));
  return m + centre + RAD * 102.9372 + Math.PI;
}
const declination = (l) => Math.asin(Math.sin(OBLIQUITY) * Math.sin(l));
const transit = (ds, m, l) => J2000 + ds + 0.0053 * Math.sin(m) - 0.0069 * Math.sin(2 * l);
const approxTransit = (ht, lw, n) => J0 + (ht + lw) / (2 * Math.PI) + n;

/** The angles of the sun, in degrees above the horizon, that the day turns on. */
export const ANGLES = {
  // The top of the sun touching the horizon, refraction included.
  horizon: -0.833,
  // Golden light: the sun low but still up.
  golden: 6,
  // Blue light: below the horizon, between these.
  blueStart: -4,
  blueEnd: -6,
};

/**
 * When the sun crosses each of `ANGLES` on the day of `date`, at a place
 * (degrees, north and east positive): `{ angle: { rise, set } }` as Dates,
 * either null when the sun never gets there that day, and `noon`.
 */
export function sunTimes(date, lat, lon) {
  const lw = RAD * -lon;
  const phi = RAD * lat;
  // From local midday, so the day asked for is the day answered.
  const d = toDays(new Date(date.getFullYear(), date.getMonth(), date.getDate(), 12));
  const n = Math.round(d - J0 - lw / (2 * Math.PI));
  const ds = approxTransit(0, lw, n);
  const m = meanAnomaly(ds);
  const l = eclipticLongitude(m);
  const dec = declination(l);
  const noon = transit(ds, m, l);
  const out = { noon: fromJulian(noon) };
  for (const [name, degrees] of Object.entries(ANGLES)) {
    const cosine = (Math.sin(degrees * RAD) - Math.sin(phi) * Math.sin(dec))
      / (Math.cos(phi) * Math.cos(dec));
    if (cosine < -1 || cosine > 1) {
      out[name] = { rise: null, set: null };
      continue;
    }
    const set = transit(approxTransit(Math.acos(cosine), lw, n), m, l);
    out[name] = { rise: fromJulian(noon - (set - noon)), set: fromJulian(set) };
  }
  return out;
}

/**
 * The golden and blue hours of a day, morning and evening, as `{ from, to }`
 * pairs of Dates (null where the sun does not get there). Golden is from the
 * horizon to six degrees up, blue from four to six degrees down.
 */
export function lightOfDay(date, lat, lon) {
  const t = sunTimes(date, lat, lon);
  const span = (from, to) => (from && to ? { from, to } : null);
  return {
    noon: t.noon,
    sunrise: t.horizon.rise,
    sunset: t.horizon.set,
    morningBlue: span(t.blueEnd.rise, t.blueStart.rise),
    morningGolden: span(t.horizon.rise, t.golden.rise),
    eveningGolden: span(t.golden.set, t.horizon.set),
    eveningBlue: span(t.blueStart.set, t.blueEnd.set),
  };
}

/** A duration in words-free units: 1/250 s, 0.4 s, 2.5 s, 4 min 16 s, 1 h 8 min. */
export function duration(seconds) {
  if (!(seconds > 0)) return '—';
  // Fractions while they are whole ones, as cameras write them; 0.3 s and
  // up in tenths.
  if (seconds < 0.29) return `1/${Math.round(1 / seconds)} s`;
  if (seconds < 0.95) return `${Math.round(seconds * 10) / 10} s`;
  if (seconds < 60) return `${seconds < 10 ? Math.round(seconds * 10) / 10 : Math.round(seconds)} s`;
  const whole = Math.round(seconds);
  if (whole < 3600) {
    const s = whole % 60;
    return `${Math.floor(whole / 60)} min${s ? ` ${s} s` : ''}`;
  }
  const minutes = Math.round(whole / 60);
  const m = minutes % 60;
  return `${Math.floor(minutes / 60)} h${m ? ` ${m} min` : ''}`;
}
