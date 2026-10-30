/** Oklab to sRGB for drawing, the same space `core/src/wheels.rs` grades in,
 *  so the colour a wheel shows under its point is the colour it pushes. */

const encode = (v) => {
  const c = Math.min(Math.max(v, 0), 1);
  return c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055;
};

/** An Oklab colour as 8-bit sRGB, clipped into gamut. */
export function oklabToSrgb(l, a, b) {
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ].map((v) => Math.round(encode(v) * 255));
}

/** The colour of an Oklab hue (degrees) at a chroma, at a middling lightness. */
export function hueColour(hue, chroma = 0.12, lightness = 0.72) {
  const r = (hue * Math.PI) / 180;
  const [R, G, B] = oklabToSrgb(lightness, Math.cos(r) * chroma, Math.sin(r) * chroma);
  return `rgb(${R} ${G} ${B})`;
}
