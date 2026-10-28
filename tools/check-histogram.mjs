// The histogram and clipping arithmetic, without a browser:
//   node tools/check-histogram.mjs
import { analyse, clipMask, measure, BLOWN } from '../web/src/lib/histogram.js';

globalThis.ImageData ??= class { constructor(w, h) { this.width = w; this.height = h; this.data = new Uint8ClampedArray(w * h * 4); } };
let failed = 0;
const check = (name, ok) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`); if (!ok) failed++; };

const px = (list) => {
  const img = new ImageData(list.length, 1);
  list.forEach((p, i) => img.data.set([...p, 255], i * 4));
  return img;
};

const img = px([[255, 10, 10], [0, 0, 0], [128, 128, 128], [20, 30, 40]]);
const a = analyse(img);
check('a blown red channel counts as blown', a.blown === 0.25);
check('pure black counts as crushed', a.crushed === 0.25);
check('every pixel lands in each channel histogram once', a.r.reduce((s, v) => s + v, 0) === 4);
check('mid grey sits at its own level in luminance', a.l[128] === 1);
const dark = analyse(px([[1, 0, 200]]));
check('one dark channel is not crushed', dark.crushed === 0);
const m = clipMask(img).data;
check('blown is marked red', m[0] === 255 && m[3] > 0);
check('crushed is marked blue', m[6] === 255 && m[7] > 0);
check('the rest is left clear', m[11] === 0 && m[15] === 0);
check('the threshold allows for dither', BLOWN === 254);

// Exposure and sharpness, for judging a whole picture.
const field = (w, h, fn) => {
  const out = new ImageData(w, h);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) out.data.set([...fn(x, y), 255], (y * w + x) * 4);
  return out;
};
const grey = measure(field(64, 64, () => [128, 128, 128]));
check('a flat grey has no detail', grey.sharpness === 0);
check('a flat grey is half bright', Math.abs(grey.brightness - 128 / 255) < 1e-6);
const half = measure(field(64, 64, (x) => (x < 32 ? [255, 255, 255] : [0, 0, 0])));
check('half white, half black is half blown and half crushed', half.blown === 0.5 && half.crushed === 0.5);
const stripes = (period) => field(64, 64, (x) => {
  const v = Math.sin((2 * Math.PI * x) / period) * 100 + 128;
  return [v, v, v];
});
check('fine detail measures sharper than coarse', measure(stripes(8)).sharpness > measure(stripes(32)).sharpness * 4);
const box = (img) => {
  const out = new ImageData(img.width, img.height);
  for (let i = 0; i < img.data.length; i += 4) {
    for (let c = 0; c < 3; c++) {
      let s = 0, n = 0;
      for (let k = -3; k <= 3; k++) { const j = i + k * 4; if (j >= 0 && j < img.data.length) { s += img.data[j + c]; n++; } }
      out.data[i + c] = s / n;
    }
    out.data[i + 3] = 255;
  }
  return out;
};
check('a blurred copy measures softer', measure(box(stripes(8))).sharpness < measure(stripes(8)).sharpness / 2);
process.exit(failed ? 1 : 0);
