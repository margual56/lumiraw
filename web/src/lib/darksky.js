/** How dark the night sky is, from David Lorenz's light pollution atlas
 *  (https://djlorenz.github.io/astronomy/lp/, 2025 data): the artificial
 *  brightness straight up, modelled from satellite measures of the lights on
 *  the ground, every 1/120° (about 1 km) from 65° S to 75° N. The atlas
 *  ships it as gzipped tiles of 5° by 5°; `decode`, `zoneOf` and `pickSpots`
 *  are pure, so `tools/check-calc.mjs` can run them. */

import { RAD, between, wrap360 } from './geo.js';
import { lastAsked, remoteFetch } from './remote.js';

const ATLAS = 'https://djlorenz.github.io/astronomy/binary_tiles/2025';
/** The atlas's colour overlay, for a map: tiles 1024 px across, to zoom 8. */
export const OVERLAY = 'https://djlorenz.github.io/astronomy/image_tiles/tiles2025/tile_{z}_{x}_{y}.png';
export const ATLAS_PAGE = 'https://djlorenz.github.io/astronomy/lp/';
export const BORTLE_PAGE = 'https://djlorenz.github.io/astronomy/lp/bortle.html';

/** Points a side of a tile, and the degrees a point. */
const SIDE = 600;
const CELL = 1 / 120;
/** The atlas's south and north edges. */
const SOUTH = -65;
const NORTH = 75;

/**
 * The atlas's zones, by the artificial brightness over the natural sky's: the
 * upper bound, the zone's name, its colour on the overlay, and the Bortle
 * class most often seen in it. The atlas measures straight up and Bortle the
 * whole sky by eye, so that last is a guess: from the author's comparison
 * with the U.S. National Park Service's surveys, where yellow is most often
 * Bortle 5 and orange 6.
 */
export const ZONES = [
  [0.01, '0', [0, 0, 0], 1],
  [0.06, '1a', [34, 34, 34], 1],
  [0.11, '1b', [66, 66, 66], 2],
  [0.19, '2a', [20, 47, 114], 3],
  [0.33, '2b', [33, 84, 216], 3],
  [0.58, '3a', [15, 87, 20], 4],
  [1.0, '3b', [31, 161, 42], 4],
  [1.73, '4a', [110, 100, 30], 5],
  [3.0, '4b', [184, 166, 37], 5],
  [5.2, '5a', [191, 100, 30], 6],
  [9.0, '5b', [253, 150, 80], 6],
  [15.59, '6a', [251, 90, 73], 7],
  [27.0, '6b', [251, 153, 138], 7],
  [46.77, '7a', [160, 160, 160], 8],
  [Infinity, '7b', [242, 242, 242], 9],
];

/** What the sky is like at an artificial brightness `ratio`: zone, colour, Bortle and mag/arcsec². */
export function zoneOf(ratio) {
  const [, zone, colour, bortle] = ZONES.find(([below]) => ratio < below);
  // The natural sky taken as 22.0 mag/arcsec², the artificial light on top.
  const sqm = 22 - (5 * Math.log(1 + ratio)) / Math.log(100);
  return { ratio, zone, colour, bortle, sqm };
}

/**
 * A tile's brightness ratios, south to north and west to east (row-major,
 * `SIDE` by `SIDE`), from its unzipped bytes: the first point in two bytes,
 * then each point as a change on the one before it, the first column going
 * north and each row going east from it; and each value stored as an
 * exponent, as the atlas's own page decodes it.
 */
export function decode(bytes) {
  const out = new Float32Array(SIDE * SIDE);
  let start = 128 * bytes[0] + bytes[1];
  for (let row = 0; row < SIDE; row++) {
    if (row > 0) start += bytes[SIDE * row + 1];
    let value = start;
    for (let col = 0; col < SIDE; col++) {
      if (col > 0) value += bytes[SIDE * row + 1 + col];
      out[row * SIDE + col] = (5 / 195) * (Math.exp(0.0195 * value) - 1);
    }
  }
  return out;
}

/**
 * The atlas's point a place falls on, or null outside the atlas: its tile,
 * numbered from 1 as the atlas does (`x`, `y`, and `key` for the file), and
 * where in the tile's ratios it is (`index`).
 */
function cellOf(lat, lon) {
  if (lat < SOUTH || lat >= NORTH) return null;
  const east = wrap360(lon + 180);
  const x = Math.floor(east / 5) + 1;
  const y = Math.floor((lat - SOUTH) / 5) + 1;
  const col = Math.min(SIDE - 1, Math.floor((east - 5 * (x - 1)) / CELL));
  const row = Math.min(SIDE - 1, Math.floor((lat - SOUTH - 5 * (y - 1)) / CELL));
  return { x, y, key: `${x}_${y}`, index: row * SIDE + col };
}

/** Decoded tiles kept, 1.4 MB each: a search's worth and the spot's. */
const KEEP = 12;

/** A tile's ratios, fetched and unzipped once while it stays among the last `KEEP` asked for. */
const tile = lastAsked(KEEP, async (key) => {
  const response = await remoteFetch(`${ATLAS}/binary_tile_${key}.dat.gz`);
  if (!response.ok) throw new Error(String(response.status));
  const unzipped = response.body.pipeThrough(new DecompressionStream('gzip'));
  return decode(new Int8Array(await new Response(unzipped).arrayBuffer()));
});

/** The sky over a place, or null outside the atlas. */
export async function skyAt(lat, lon) {
  const cell = cellOf(lat, lon);
  return cell && zoneOf((await tile(cell.key))[cell.index]);
}

/** The same, for drawing a sky: outside the atlas, or if it can't be read, as if unlit. */
export const skyOrUnlit = (lat, lon) => skyAt(lat, lon).then((sky) => sky ?? UNLIT, () => UNLIT);
const UNLIT = { ratio: 0, zone: null };

/**
 * The darkest places out of `points` (`{ lat, lon, ratio, km }`), `count` of
 * them, at least `apart` km from one another: by how dark the sky is, to a
 * twentieth of a magnitude (closer than that the eye can't tell), and the
 * nearest first among the equally dark.
 */
export function pickSpots(points, { count = 5, apart }) {
  const ranked = points
    .map((p) => ({ ...p, ...zoneOf(p.ratio) }))
    .sort((a, b) => Math.round(b.sqm * 20) - Math.round(a.sqm * 20) || a.km - b.km);
  const chosen = [];
  for (const p of ranked) {
    if (chosen.every((c) => between(c.lat, c.lon, p.lat, p.lon).km >= apart)) chosen.push(p);
    if (chosen.length === count) break;
  }
  return chosen;
}

/**
 * The darkest places on land within `km` of a spot, each with how far and
 * which way it is from it. `land(lat, lon)` says whether a place is on land
 * (the sea is dark, and no place to stand).
 */
export async function darkSpots(lat, lon, km, land) {
  const kmLat = 111.2;
  const kmLon = 111.2 * Math.max(0.05, Math.cos(lat * RAD));
  const south = Math.max(SOUTH, lat - km / kmLat);
  const north = Math.min(NORTH - CELL, lat + km / kmLat);
  // Every point of the atlas within reach (at its centre, half a cell in),
  // measured flat: over 150 km the earth's curve is lost in the rounding.
  const near = [];
  for (let la = Math.floor(south / CELL) * CELL + CELL / 2; la <= north; la += CELL) {
    for (let lo = Math.floor((lon - km / kmLon) / CELL) * CELL + CELL / 2; lo <= lon + km / kmLon; lo += CELL) {
      const away = Math.hypot((la - lat) * kmLat, (lo - lon) * kmLon);
      const cell = away <= km && cellOf(la, lo);
      if (cell) near.push({ lat: la, lon: wrap360(lo + 180) - 180, km: away, cell });
    }
  }
  // Every tile they fall in, fetched together.
  const keys = [...new Set(near.map((p) => p.cell.key))];
  const tiles = new Map(await Promise.all(keys.map(async (key) => [key, await tile(key)])));
  for (const p of near) p.ratio = tiles.get(p.cell.key)[p.cell.index];
  // The land checked only for the darkest, not every point.
  const darkest = near.sort((a, b) => a.ratio - b.ratio).slice(0, 4000).filter((p) => land(p.lat, p.lon));
  return pickSpots(darkest, { apart: Math.max(4, km / 4) })
    .map(({ cell, ...spot }) => ({ ...spot, ...between(lat, lon, spot.lat, spot.lon) }));
}
