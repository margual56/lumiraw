/**
 * Does the committed scoring module, with the committed weights, judge
 * pictures the way NIMA should? Generated pictures, so nothing needs fetching:
 * a blurred or noisy copy must score worse on the technical mark than the
 * sharp one, a shifted copy must look like the original, and every mark must
 * be a mark.
 */
import { readFileSync } from 'node:fs';

const ROOT = new URL('../web/src/lib/wasm/', import.meta.url);
const S = 224;

let failures = 0;
const check = (what, ok) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};

const { instance } = await WebAssembly.instantiate(readFileSync(new URL('critic.wasm', ROOT)), {});
const w = instance.exports;
const withBytes = (bytes, fn) => {
  const ptr = w.cr_alloc(bytes.length);
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
  try { return fn(ptr, bytes.length); } finally { w.cr_free(ptr, bytes.length); }
};
['nima-aesthetic.bin', 'nima-technical.bin'].forEach((name, slot) => {
  const rc = withBytes(readFileSync(new URL(`critic/${name}`, ROOT)), (p, n) => w.cr_load(slot, p, n));
  check(`${name} loads`, rc === 0);
});

w.cr_version();
const version = new TextDecoder().decode(
  new Uint8Array(w.memory.buffer, w.cr_text_ptr(), w.cr_text_len()));
check(`the module says what it is (${version})`, version.startsWith('LumiRaw '));

/** [aesthetic, technical] means, and the aesthetic features. */
function score(px) {
  return withBytes(px, (p) => {
    const out = [0, 1].map((slot) => {
      w.cr_score(slot, p, S, S);
      return new Float32Array(w.memory.buffer, w.cr_out_ptr(), w.cr_out_len()).slice();
    });
    return { a: out[0][0], t: out[1][0], std: [out[0][1], out[1][1]], feature: out[0].slice(2) };
  });
}

// A sky, a sun, ground with texture and a post with a grain: a little of
// everything the networks look at.
function scene() {
  const px = new Uint8Array(S * S * 4);
  for (let y = 0; y < S; y += 1) {
    for (let x = 0; x < S; x += 1) {
      const sky = y < 120;
      let r = sky ? 90 + y * 0.8 : 60 + 30 * Math.sin(x * 0.3) * Math.sin(y * 0.25);
      let g = sky ? 140 + y * 0.6 : 110 + 25 * Math.sin(x * 0.17 + y * 0.31);
      let b = sky ? 220 - y * 0.3 : 50;
      if (Math.hypot(x - 150, y - 80) < 30) [r, g, b] = [240, 200, 90];
      if (x > 30 && x < 70 && y > 70 && y < 200) [r, g, b] = [(x + y) % 9 < 2 ? 60 : 120, 70, 40];
      px.set([r, g, b, 255], (y * S + x) * 4);
    }
  }
  return px;
}

/** A box blur `radius` wide, across then down. */
function blur(src, radius) {
  let out = Uint8Array.from(src);
  for (const [dx, dy] of [[1, 0], [0, 1]]) {
    const from = Uint8Array.from(out);
    for (let y = 0; y < S; y += 1) {
      for (let x = 0; x < S; x += 1) {
        for (let c = 0; c < 3; c += 1) {
          let sum = 0;
          let n = 0;
          for (let k = -radius; k <= radius; k += 1) {
            const xx = x + k * dx;
            const yy = y + k * dy;
            if (xx < 0 || yy < 0 || xx >= S || yy >= S) continue;
            sum += from[(yy * S + xx) * 4 + c];
            n += 1;
          }
          out[(y * S + x) * 4 + c] = sum / n;
        }
      }
    }
  }
  return out;
}

let seed = 1;
const random = () => ((seed = (seed * 1103515245 + 12345) >>> 0) / 2 ** 32);
const noise = (src, amount) => src.map((v, i) => (i % 4 === 3 ? v
  : Math.max(0, Math.min(255, v + (random() - 0.5) * 2 * amount))));

const shift = (src, dx) => {
  const out = new Uint8Array(src.length);
  for (let y = 0; y < S; y += 1) {
    for (let x = 0; x < S; x += 1) {
      const from = (y * S + Math.min(S - 1, Math.max(0, x + dx))) * 4;
      out.set(src.subarray(from, from + 4), (y * S + x) * 4);
    }
  }
  return out;
};

const cosine = (a, b) => {
  let d = 0;
  let x = 0;
  let y = 0;
  for (let i = 0; i < a.length; i += 1) { d += a[i] * b[i]; x += a[i] ** 2; y += b[i] ** 2; }
  return d / Math.sqrt(x * y);
};

const sharp = score(scene());
const soft = score(blur(scene(), 4));
const softer = score(blur(scene(), 8));
const grainy = score(noise(scene(), 50));
const again = score(scene());
const moved = score(shift(scene(), 8));
const flat = score(new Uint8Array(S * S * 4).fill(128));

const show = (s) => `${s.a.toFixed(2)}/${s.t.toFixed(2)}`;
console.log(`     sharp ${show(sharp)}, blurred ${show(soft)} and ${show(softer)}, noisy ${show(grainy)}`);

const marks = [sharp, soft, softer, grainy, moved, flat];
check('every mark is between 1 and 10',
  marks.every((s) => [s.a, s.t].every((v) => v >= 1 && v <= 10)));
check('every spread is a spread', marks.every((s) => s.std.every((v) => v > 0 && v < 4.5)));
check('the same picture scores the same', again.a === sharp.a && again.t === sharp.t);
check('the features are MobileNet\'s 1024', sharp.feature.length === 1024);
check('a blurred copy is technically worse', soft.t < sharp.t - 0.2);
check('a more blurred copy is worse again', softer.t < soft.t - 0.2);
check('a noisy copy is technically worse', grainy.t < sharp.t - 0.1);
check('a blurred copy also looks worse', soft.a < sharp.a);
check('a shifted copy has nearly the same features',
  cosine(moved.feature, sharp.feature) > 0.95);
check('a flat grey picture is the furthest from the scene',
  cosine(flat.feature, sharp.feature) < cosine(moved.feature, sharp.feature) - 0.2);

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
