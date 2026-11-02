/** How dark the night sky is, from David Lorenz's light pollution atlas
 *  (https://djlorenz.github.io/astronomy/lp/, 2025 data): the artificial
 *  brightness straight up, modelled from satellite measures of the lights on
 *  the ground, every 1/120° (about 1 km) from 65° S to 75° N. The atlas
 *  ships it as gzipped tiles of 5° by 5°; `decode`, `zoneOf` and `pickSpots`
 *  are pure, so `tools/check-calc.mjs` can run them. */

const RAD = Math.PI / 180;
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

/** The tile a place falls in, numbered from 1 as the atlas does, or null outside it. */
function tileOf(lat, lon) {
  if (lat < SOUTH || lat >= NORTH) return null;
  const east = (((lon + 180) % 360) + 360) % 360;
  return { x: Math.floor(east / 5) + 1, y: Math.floor((lat - SOUTH) / 5) + 1 };
}

const cache = new Map();
/** A tile's ratios, fetched and unzipped once. */
function tile(x, y) {
  const key = `${x}_${y}`;
  if (!cache.has(key)) {
    cache.set(key, (async () => {
      const response = await fetch(`${ATLAS}/binary_tile_${key}.dat.gz`);
      if (!response.ok) throw new Error(String(response.status));
      const unzipped = response.body.pipeThrough(new DecompressionStream('gzip'));
      return decode(new Int8Array(await new Response(unzipped).arrayBuffer()));
    })().catch((error) => {
      cache.delete(key);
      throw error;
    }));
  }
  return cache.get(key);
}

/** The sky over a place, or null outside the atlas. */
export async function skyAt(lat, lon) {
  const at = tileOf(lat, lon);
  if (!at) return null;
  const ratios = await tile(at.x, at.y);
  const east = (((lon + 180) % 360) + 360) % 360;
  const col = Math.min(SIDE - 1, Math.floor((east - 5 * (at.x - 1)) / CELL));
  const row = Math.min(SIDE - 1, Math.floor((lat - SOUTH - 5 * (at.y - 1)) / CELL));
  return zoneOf(ratios[row * SIDE + col]);
}

/** Kilometres between two places, and the bearing from the first to the second. */
export function between(lat, lon, lat2, lon2) {
  const phi = lat * RAD;
  const phi2 = lat2 * RAD;
  const dl = (lon2 - lon) * RAD;
  const a = Math.sin((phi2 - phi) / 2) ** 2 + Math.cos(phi) * Math.cos(phi2) * Math.sin(dl / 2) ** 2;
  const bearing = Math.atan2(Math.sin(dl) * Math.cos(phi2), Math.cos(phi) * Math.sin(phi2) - Math.sin(phi) * Math.cos(phi2) * Math.cos(dl));
  return { km: 12742 * Math.asin(Math.sqrt(a)), bearing: ((bearing / RAD) + 360) % 360 };
}

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
 * The darkest places on land within `km` of a spot. `land(lat, lon)` says
 * whether a place is on land (the sea is dark, and no place to stand).
 */
export async function darkSpots(lat, lon, km, land) {
  const reachLat = km / 111.2;
  const reachLon = km / (111.2 * Math.max(0.05, Math.cos(lat * RAD)));
  const south = Math.max(SOUTH, lat - reachLat);
  const north = Math.min(NORTH - CELL, lat + reachLat);
  // Every tile the circle touches, fetched together.
  const wanted = new Map();
  for (let la = south; la <= north + 5; la += 5) {
    for (let lo = lon - reachLon; lo <= lon + reachLon + 5; lo += 5) {
      const at = tileOf(Math.min(la, north), Math.min(lo, lon + reachLon));
      if (at) wanted.set(`${at.x}_${at.y}`, at);
    }
  }
  const tiles = new Map(await Promise.all([...wanted].map(async ([key, at]) => [key, await tile(at.x, at.y)])));
  const points = [];
  // Every point of the atlas within reach: its centre, 1/240° in from the edge of its cell.
  for (let la = Math.floor(south / CELL) * CELL + CELL / 2; la <= north; la += CELL) {
    for (let lo = Math.floor((lon - reachLon) / CELL) * CELL + CELL / 2; lo <= lon + reachLon; lo += CELL) {
      const at = tileOf(la, lo);
      const ratios = at && tiles.get(`${at.x}_${at.y}`);
      if (!ratios) continue;
      const { km: away } = between(lat, lon, la, lo);
      if (away > km) continue;
      const east = (((lo + 180) % 360) + 360) % 360;
      const col = Math.min(SIDE - 1, Math.floor((east - 5 * (at.x - 1)) / CELL));
      const row = Math.min(SIDE - 1, Math.floor((la - SOUTH - 5 * (at.y - 1)) / CELL));
      points.push({ lat: la, lon: ((lo + 540) % 360) - 180, ratio: ratios[row * SIDE + col], km: away });
    }
  }
  // The land checked only for the darkest, not every point.
  const darkest = points.sort((a, b) => a.ratio - b.ratio).slice(0, 4000).filter((p) => land(p.lat, p.lon));
  return pickSpots(darkest, { apart: Math.max(4, km / 4) });
}
