#!/usr/bin/env node
/**
 * Bake the night sky for the golden hour calculator, from Olaf Frohn's
 * d3-celestial (BSD-3-Clause, https://github.com/ofrohn/d3-celestial):
 *
 *   node tools/bake-sky.mjs
 *
 * Its stars are from the Hipparcos compilation XHIP, the constellations'
 * lines and names after the IAU's, and the Milky Way the outlines of
 * Jose R. Vieira's catalogue, in five steps of brightness. All of them at
 * J2000; the sky has moved a third of a degree since, less than a star's dot.
 *
 * The output is `web/src/lib/sky-data.js`.
 */
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE = 'https://raw.githubusercontent.com/ofrohn/d3-celestial/7e720a3de062059d4c5400a379146a601d9010e0/data';
/** The faintest star kept: the eye's limit under a good country sky. */
const FAINTEST = 5;
/** The faintest star named. */
const NAMED = 2;
/** The Milky Way's grid, in degrees. */
const GRID = 2;

const get = async (name) => (await fetch(`${SOURCE}/${name}`)).json();
const [stars, names, constellations, lines, milky] = await Promise.all(
  ['stars.6.json', 'starnames.json', 'constellations.json', 'constellations.lines.json', 'mw.json'].map(get));

// GeoJSON's longitudes, -180 to 180, back to right ascensions.
const ra = (lon) => (lon + 360) % 360;
const r2 = (x) => Math.round(x * 100) / 100;

const kept = stars.features
  .filter((f) => f.properties.mag <= FAINTEST)
  .sort((a, b) => a.properties.mag - b.properties.mag);
const STARS = kept.flatMap((f) => {
  const [lon, dec] = f.geometry.coordinates;
  const bv = Number.parseFloat(f.properties.bv);
  return [r2(ra(lon)), r2(dec), Math.round(f.properties.mag * 10) / 10, Number.isFinite(bv) ? Math.round(bv * 10) / 10 : 0.6];
});

const NAMES = kept.flatMap((f, i) => {
  const name = names[f.id];
  // Polaris too, faint as it is, for it is the one everyone looks for.
  if (!name?.name || (f.properties.mag > NAMED && name.name !== 'Polaris')) return [];
  return [[i, name.name, name.es || name.name]];
});

const linesOf = new Map(lines.features.map((f) => [f.id, f.geometry.coordinates]));
const CONSTELLATIONS = constellations.features.map((f) => {
  const p = f.properties;
  return {
    id: f.id,
    // In English the Latin names, as photographers and astronomers use them; Spanish has its own.
    en: p.name,
    // The Spanish names come with narrow spaces between words.
    es: (p.es || p.name).replace(/ /g, ' '),
    rank: Number(p.rank),
    at: [r2(ra(f.geometry.coordinates[0])), r2(f.geometry.coordinates[1])],
    lines: (linesOf.get(f.id) ?? []).map((line) => line.flatMap(([lon, dec]) => [r2(ra(lon)), r2(dec)])),
  };
});

// The Milky Way on a grid, each cell the number of its outlines it falls
// within: 0 off it, 5 in its brightest clouds. Even-odd along each meridian,
// the jumps from 180° to -180° left out.
const levels = milky.features.map((f) => f.geometry.coordinates.flat());
function inside(rings, lon, lat) {
  let odd = false;
  for (const ring of rings) {
    for (let i = 1; i < ring.length; i++) {
      const [x1, y1] = ring[i - 1];
      const [x2, y2] = ring[i];
      if (Math.abs(x2 - x1) > 180 || (x1 > lon) === (x2 > lon)) continue;
      if (y1 + ((y2 - y1) * (lon - x1)) / (x2 - x1) > lat) odd = !odd;
    }
  }
  return odd;
}
let MILKY = '';
for (let dec = -90 + GRID / 2; dec < 90; dec += GRID) {
  for (let r = GRID / 2; r < 360; r += GRID) {
    const lon = r > 180 ? r - 360 : r;
    MILKY += levels.reduce((n, rings) => n + inside(rings, lon, dec), 0);
  }
}

const out = `/** The night sky, baked by tools/bake-sky.mjs from d3-celestial (BSD-3-Clause,
 *  Olaf Frohn): don't edit it by hand. */

/** Stars to magnitude ${FAINTEST}, brightest first, four numbers each: right
 *  ascension and declination (degrees, J2000), magnitude, and B−V colour. */
export const STARS = ${JSON.stringify(STARS)};

/** The brightest stars' names, by their place in STARS: [index, English, Spanish]. */
export const NAMES = ${JSON.stringify(NAMES)};

/** The 88 constellations: names, rank (1 the best known, 3 the faintest),
 *  where to write the name, and their lines as runs of [ra, dec, ra, dec…]. */
export const CONSTELLATIONS = ${JSON.stringify(CONSTELLATIONS)};

/** The Milky Way every ${GRID}°, from the south pole up, each row from 0h of right
 *  ascension east: how many of its five outlines each cell is inside. */
export const MILKY_GRID = ${GRID};
export const MILKY = '${MILKY}';
`;
const file = join(dirname(fileURLToPath(import.meta.url)), '../web/src/lib/sky-data.js');
writeFileSync(file, out);
console.log(`sky-data.js: ${kept.length} stars, ${NAMES.length} names, ${CONSTELLATIONS.length} constellations, ${out.length} bytes`);
