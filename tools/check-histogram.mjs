// The histogram and clipping arithmetic, without a browser:
//   node tools/check-histogram.mjs
import { analyse, clipMask, BLOWN } from '../web/src/lib/histogram.js';

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
process.exit(failed ? 1 : 0);
