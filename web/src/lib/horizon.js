/** The horizon from a spot: how high the land stands against the sky in
 *  every direction, from the terrain's heights, so the sun can be seen going
 *  behind a hill before it sets. Heights come from the Terrain Tiles on AWS
 *  (Mapzen's, from SRTM and others), which need no account; buildings and
 *  trees are not in them. `skyline` is pure, over any height lookup, so
 *  `tools/check-calc.mjs` can run it. */

import { RAD, wrap360 } from './geo.js';
import { lastAsked, remoteFetch } from './remote.js';

const EARTH = 6371000;
/** How much the air bends a sight line over the curve of the earth. */
const REFRACTION = 0.13;
/** An eye above the ground. */
const EYE = 1.7;

/** Directions every half degree, clockwise from north. */
export const STEP = 0.5;
/** How far each layer of land reaches, in metres: the near hills drawn over the far ones. */
export const LAYERS = [1500, 5000, 15000, 60000];

/** The tiles' zoom by distance: finer close by, where a little height is a large angle. */
const ZOOMS = [[2500, 13], [12000, 11], [60000, 9]];

/** Distances along each bearing, growing as they go (a step is worth less
 *  the farther it is), with what each needs that is the same on every bearing. */
const MARCH = (() => {
  const out = [];
  for (let d = 100; d <= LAYERS.at(-1); d += Math.max(15, d * 0.012)) {
    const delta = d / EARTH;
    out.push({
      d, sin: Math.sin(delta), cos: Math.cos(delta),
      // The earth's curve under the sight line, less what the air bends it back.
      drop: (d * d * (1 - REFRACTION)) / (2 * EARTH),
      zoom: ZOOMS.find(([reach]) => d <= reach)[1],
    });
  }
  return out;
})();

/**
 * The land's angle above level in every direction (degrees, `STEP` apart),
 * for each of `LAYERS`: how high it stands within that distance, the earth's
 * curve taken off. `height(lat, lon, zoom)` gives metres above the sea, or
 * null where it is not known.
 */
export function skyline(height, lat, lon) {
  const phi = lat * RAD;
  const sinPhi = Math.sin(phi);
  const cosPhi = Math.cos(phi);
  const ground = Math.max(0, height(lat, lon, 13) ?? 0);
  const eye = ground + EYE;
  const count = Math.round(360 / STEP);
  const layers = LAYERS.map(() => new Float32Array(count).fill(-90));
  for (let i = 0; i < count; i++) {
    const theta = i * STEP * RAD;
    const sinTheta = Math.sin(theta);
    const cosTheta = Math.cos(theta);
    let top = -90;
    let layer = 0;
    for (const step of MARCH) {
      while (step.d > LAYERS[layer]) {
        layers[layer][i] = top;
        layer += 1;
      }
      // The point `d` metres along the bearing, on the sphere.
      const sinPhi2 = sinPhi * step.cos + cosPhi * step.sin * cosTheta;
      const lambda = Math.atan2(sinTheta * step.sin * cosPhi, step.cos - sinPhi * sinPhi2);
      const h = height(Math.asin(sinPhi2) / RAD, lon + lambda / RAD, step.zoom);
      if (h == null) continue;
      // The sea is level, whatever lies under it.
      const angle = Math.atan2(Math.max(0, h) - eye - step.drop, step.d) / RAD;
      if (angle > top) top = angle;
    }
    for (; layer < LAYERS.length; layer++) layers[layer][i] = top;
  }
  return { ground, layers };
}

/** How high the land stands at `bearing`, between the directions worked out. */
export function landAt(line, bearing) {
  const at = wrap360(bearing) / STEP;
  const i = Math.floor(at);
  const k = at - i;
  return line[i % line.length] * (1 - k) + line[(i + 1) % line.length] * k;
}

// The tiles
const TILES = 'https://s3.amazonaws.com/elevation-tiles-prod/terrarium';
const SIZE = 256;
/** Decoded tiles kept, some 260 KB each: the last few places' worth. */
const KEEP = 64;

let scratch = null;

/** A tile's heights, in metres, decoded from its colours (Terrarium's
 *  encoding); fetched once while it stays among the last `KEEP` asked for. */
const tile = lastAsked(KEEP, async (key) => {
  const response = await remoteFetch(`${TILES}/${key}.png`);
  if (!response.ok) throw new Error(String(response.status));
  const bitmap = await createImageBitmap(await response.blob(), { colorSpaceConversion: 'none', premultiplyAlpha: 'none' });
  if (!scratch) {
    scratch = document.createElement('canvas');
    scratch.width = SIZE;
    scratch.height = SIZE;
  }
  const context = scratch.getContext('2d', { willReadFrequently: true });
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  const { data } = context.getImageData(0, 0, SIZE, SIZE);
  const heights = new Float32Array(SIZE * SIZE);
  for (let i = 0; i < heights.length; i++) {
    heights[i] = data[i * 4] * 256 + data[i * 4 + 1] + data[i * 4 + 2] / 256 - 32768;
  }
  return heights;
});

/** Where a place falls on the tiles of `z`, in pixels. */
function pixel(lat, lon, z) {
  const scale = SIZE * 2 ** z;
  const phi = lat * RAD;
  return [((lon + 180) / 360) * scale, ((1 - Math.log(Math.tan(phi) + 1 / Math.cos(phi)) / Math.PI) / 2) * scale];
}

/**
 * The terrain's heights about a spot, fetching the tiles they take all at
 * once: for each `[reach, zoom]` of `levels`, the tiles of that zoom within
 * `reach` metres. Gives `height(lat, lon, zoom)`, in metres above the sea, or
 * null outside what was fetched.
 */
export async function heightsAround(lat, lon, levels) {
  // Per zoom, the tiles by their number across and down (x + y · 2^z).
  const tiles = new Map(levels.map(([, z]) => [z, new Map()]));
  const jobs = [];
  for (const [reach, z] of levels) {
    const across = reach / (111320 * Math.cos(lat * RAD));
    const [x0, y0] = pixel(lat + reach / 111320, lon - across, z);
    const [x1, y1] = pixel(lat - reach / 111320, lon + across, z);
    const n = 2 ** z;
    for (let x = Math.floor(x0 / SIZE); x <= Math.floor(x1 / SIZE); x++) {
      for (let y = Math.max(0, Math.floor(y0 / SIZE)); y <= Math.min(n - 1, Math.floor(y1 / SIZE)); y++) {
        const wrapped = ((x % n) + n) % n;
        const index = wrapped + y * n;
        if (tiles.get(z).has(index)) continue;
        tiles.get(z).set(index, null);
        jobs.push(tile(`${z}/${wrapped}/${y}`).then((heights) => tiles.get(z).set(index, heights)));
      }
    }
  }
  await Promise.all(jobs);
  // Heights between the four pixels about a place.
  return (la, lo, z) => {
    const n = 2 ** z;
    const ofZoom = tiles.get(z);
    if (!ofZoom) return null;
    const [px, py] = pixel(la, lo, z);
    const fx = px - 0.5;
    const fy = py - 0.5;
    const x = Math.floor(fx);
    const y = Math.floor(fy);
    const at = (gx, gy) => {
      const tx = ((Math.floor(gx / SIZE) % n) + n) % n;
      const heights = ofZoom.get(tx + Math.floor(gy / SIZE) * n);
      return heights ? heights[(((gy % SIZE) + SIZE) % SIZE) * SIZE + (((gx % SIZE) + SIZE) % SIZE)] : null;
    };
    const a = at(x, y);
    const b = at(x + 1, y);
    const c = at(x, y + 1);
    const d = at(x + 1, y + 1);
    // At a tile's edge with its neighbour not fetched: any corner known.
    if (a == null || b == null || c == null || d == null) return a ?? b ?? c ?? d ?? null;
    const kx = fx - x;
    const ky = fy - y;
    return (a * (1 - kx) + b * kx) * (1 - ky) + (c * (1 - kx) + d * kx) * ky;
  };
}

/** The horizon from a spot: a few dozen tiles at most. */
export async function horizonFrom(lat, lon) {
  return skyline(await heightsAround(lat, lon, ZOOMS), lat, lon);
}
