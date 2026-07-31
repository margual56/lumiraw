/** Which parts of a picture each tone zone reaches. */

/** sRGB byte to linear light, the inverse of `ops::srgb_encode_scalar`. */
const LINEAR = new Float32Array(256);
for (let i = 0; i < 256; i += 1) {
  const x = i / 255;
  LINEAR[i] = x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
}

/** Oklab lightness of one linear RGB triple: `ops::rgb_to_oklab_px`, L only. */
function lightness(r, g, b) {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
}

/** `ops::smoothstep`. */
function smoothstep(edge0, edge1, x) {
  const t = Math.min(Math.max((x - edge0) / Math.max(edge1 - edge0, 1e-9), 0), 1);
  return t * t * (3 - 2 * t);
}

/** How strongly one lightness belongs to each zone: `grade.rs::zone_weights`. */
export function zoneWeights(l) {
  const shadow = 1 - smoothstep(0, 0.5, l);
  const highlight = smoothstep(0.5, 1, l);
  return [shadow, 1 - shadow - highlight, highlight];
}

/** The Oklab lightness of every pixel of an `ImageData`, as a plane. */
export function lightnessPlane(image) {
  const { data } = image;
  const out = new Float32Array(data.length / 4);
  for (let i = 0, j = 0; j < out.length; i += 4, j += 1) {
    out[j] = lightness(LINEAR[data[i]], LINEAR[data[i + 1]], LINEAR[data[i + 2]]);
  }
  return out;
}

export const ZONES = ['shadows', 'midtones', 'highlights'];

/** The strongest the mask ever gets, where a zone owns a pixel outright. */
const MASK_ALPHA = 0.5;

/** Paint one zone's reach into an RGBA buffer, red where the zone has a hold. */
export function paintMask(plane, zone, out) {
  const which = ZONES.indexOf(zone);
  if (which < 0) return out;
  const px = out.data;
  // The weight is worked out in place rather than through `zoneWeights`.
  for (let i = 0, p = 0; i < plane.length; i += 1, p += 4) {
    const l = plane[i];
    const shadow = 1 - smoothstep(0, 0.5, l);
    const highlight = smoothstep(0.5, 1, l);
    const weight = which === 0 ? shadow : which === 2 ? highlight : 1 - shadow - highlight;
    px[p] = 236;
    px[p + 1] = 58;
    px[p + 2] = 58;
    px[p + 3] = (weight * MASK_ALPHA * 255) | 0;
  }
  return out;
}
