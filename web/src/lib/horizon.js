/** The horizon from a spot: how high the land stands against the sky in
 *  every direction, from the terrain's heights, so the sun can be seen going
 *  behind a hill before it sets. Heights come from the Terrain Tiles on AWS
 *  (Mapzen's, from SRTM and others), which need no account; buildings and
 *  trees are not in them. `skyline` is pure, over any height lookup, so
 *  `tools/check-calc.mjs` can run it. */

const RAD = Math.PI / 180;
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
const zoomAt = (d) => ZOOMS.find(([reach]) => d <= reach)?.[1] ?? 9;

/**
 * The land's angle above level in every direction (degrees, `STEP` apart),
 * for each of `LAYERS`: how high it stands within that distance, the earth's
 * curve taken off. `height(lat, lon, zoom)` gives metres above the sea, or
 * null where it is not known.
 */
export function skyline(height, lat, lon) {
  const phi = lat * RAD;
  const ground = Math.max(0, height(lat, lon, 13) ?? 0);
  const eye = ground + EYE;
  const count = Math.round(360 / STEP);
  const layers = LAYERS.map(() => new Float32Array(count).fill(-90));
  // Distances growing as they go: a step is worth less the farther it is.
  const distances = [];
  for (let d = 100; d <= LAYERS.at(-1); d += Math.max(15, d * 0.012)) distances.push(d);
  for (let i = 0; i < count; i++) {
    const theta = i * STEP * RAD;
    let top = -90;
    let layer = 0;
    for (const d of distances) {
      while (d > LAYERS[layer]) {
        layers[layer][i] = top;
        layer += 1;
      }
      // The point `d` metres along the bearing, on the sphere.
      const delta = d / EARTH;
      const phi2 = Math.asin(Math.sin(phi) * Math.cos(delta) + Math.cos(phi) * Math.sin(delta) * Math.cos(theta));
      const lambda = Math.atan2(Math.sin(theta) * Math.sin(delta) * Math.cos(phi), Math.cos(delta) - Math.sin(phi) * Math.sin(phi2));
      const h = height(phi2 / RAD, lon + lambda / RAD, zoomAt(d));
      if (h == null) continue;
      // The sea is level, whatever lies under it.
      const drop = (d * d * (1 - REFRACTION)) / (2 * EARTH);
      const angle = Math.atan2(Math.max(0, h) - eye - drop, d) / RAD;
      if (angle > top) top = angle;
    }
    for (; layer < LAYERS.length; layer++) layers[layer][i] = top;
  }
  return { ground, layers };
}

/** How high the land stands at `bearing`, between the directions worked out. */
export function landAt(line, bearing) {
  const at = ((((bearing % 360) + 360) % 360) / STEP);
  const i = Math.floor(at);
  const k = at - i;
  return line[i % line.length] * (1 - k) + line[(i + 1) % line.length] * k;
}

// -- the tiles --------------------------------------------------------------------

const TILES = 'https://s3.amazonaws.com/elevation-tiles-prod/terrarium';
const SIZE = 256;

/** A tile's heights, in metres, decoded from its colours (Terrarium's encoding). */
async function tile(z, x, y) {
  const response = await fetch(`${TILES}/${z}/${x}/${y}.png`);
  if (!response.ok) throw new Error(String(response.status));
  const bitmap = await createImageBitmap(await response.blob(), { colorSpaceConversion: 'none', premultiplyAlpha: 'none' });
  const canvas = document.createElement('canvas');
  canvas.width = SIZE;
  canvas.height = SIZE;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  context.drawImage(bitmap, 0, 0);
  const { data } = context.getImageData(0, 0, SIZE, SIZE);
  const heights = new Float32Array(SIZE * SIZE);
  for (let i = 0; i < heights.length; i++) {
    heights[i] = data[i * 4] * 256 + data[i * 4 + 1] + data[i * 4 + 2] / 256 - 32768;
  }
  return heights;
}

/** Where a place falls on the tiles of `z`, in pixels. */
function pixel(lat, lon, z) {
  const scale = SIZE * 2 ** z;
  const phi = lat * RAD;
  return [((lon + 180) / 360) * scale, ((1 - Math.log(Math.tan(phi) + 1 / Math.cos(phi)) / Math.PI) / 2) * scale];
}

/** The horizon from a spot, fetching the tiles it needs: a few dozen at most, all at once. */
export async function horizonFrom(lat, lon) {
  const tiles = new Map();
  const jobs = [];
  for (const [reach, z] of ZOOMS) {
    const across = reach / (111320 * Math.cos(lat * RAD));
    const [x0, y0] = pixel(lat + reach / 111320, lon - across, z);
    const [x1, y1] = pixel(lat - reach / 111320, lon + across, z);
    const n = 2 ** z;
    for (let x = Math.floor(x0 / SIZE); x <= Math.floor(x1 / SIZE); x++) {
      for (let y = Math.max(0, Math.floor(y0 / SIZE)); y <= Math.min(n - 1, Math.floor(y1 / SIZE)); y++) {
        const wrapped = ((x % n) + n) % n;
        const key = `${z}/${wrapped}/${y}`;
        if (tiles.has(key)) continue;
        tiles.set(key, null);
        jobs.push(tile(z, wrapped, y).then((heights) => tiles.set(key, heights)));
      }
    }
  }
  await Promise.all(jobs);
  // Heights between the four pixels about a place.
  const height = (la, lo, z) => {
    const n = 2 ** z;
    const [px, py] = pixel(la, lo, z);
    const fx = px - 0.5;
    const fy = py - 0.5;
    const x = Math.floor(fx);
    const y = Math.floor(fy);
    const at = (gx, gy) => {
      const tx = ((Math.floor(gx / SIZE) % n) + n) % n;
      const heights = tiles.get(`${z}/${tx}/${Math.floor(gy / SIZE)}`);
      if (!heights) return null;
      return heights[(((gy % SIZE) + SIZE) % SIZE) * SIZE + (((gx % SIZE) + SIZE) % SIZE)];
    };
    const corners = [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)];
    if (corners.some((c) => c == null)) return corners.find((c) => c != null) ?? null;
    const kx = fx - x;
    const ky = fy - y;
    return (corners[0] * (1 - kx) + corners[1] * kx) * (1 - ky) + (corners[2] * (1 - kx) + corners[3] * kx) * ky;
  };
  return skyline(height, lat, lon);
}
