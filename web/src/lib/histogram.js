/** Where the picture's tones sit, and which of them have run out. */

/** A channel at or past this is blown, at or under `CRUSHED` is black. Not 255
 *  and 0: the export's dither moves the last level by one either way. */
export const BLOWN = 254;
export const CRUSHED = 1;

/** Counts per level for red, green, blue and luminance (Rec. */
export function analyse(image) {
  const { data } = image;
  const r = new Uint32Array(256), g = new Uint32Array(256), b = new Uint32Array(256);
  const l = new Uint32Array(256);
  let blown = 0, crushed = 0;
  const n = data.length / 4;
  for (let i = 0; i < data.length; i += 4) {
    const R = data[i], G = data[i + 1], B = data[i + 2];
    r[R]++; g[G]++; b[B]++;
    l[Math.round(0.2126 * R + 0.7152 * G + 0.0722 * B)]++;
    if (R >= BLOWN || G >= BLOWN || B >= BLOWN) blown++;
    else if (R <= CRUSHED && G <= CRUSHED && B <= CRUSHED) crushed++;
  }
  return { r, g, b, l, blown: blown / n, crushed: crushed / n };
}

/** The clipping overlay: red where a channel is blown, blue where all three
 *  are black, transparent elsewhere. Same size as the picture read. */
export function clipMask(image, out = new ImageData(image.width, image.height)) {
  const src = image.data, dst = out.data;
  for (let i = 0; i < src.length; i += 4) {
    const R = src[i], G = src[i + 1], B = src[i + 2];
    if (R >= BLOWN || G >= BLOWN || B >= BLOWN) {
      dst[i] = 255; dst[i + 1] = 40; dst[i + 2] = 40; dst[i + 3] = 220;
    } else if (R <= CRUSHED && G <= CRUSHED && B <= CRUSHED) {
      dst[i] = 40; dst[i + 1] = 110; dst[i + 2] = 255; dst[i + 3] = 220;
    } else {
      dst[i + 3] = 0;
    }
  }
  return out;
}
