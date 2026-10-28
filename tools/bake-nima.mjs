/**
 * Bake idealo's NIMA weights into the flat files `core/critic` reads.
 *
 *     node tools/bake-nima.mjs WEIGHTS_DIR
 *     node tools/bake-nima.mjs WEIGHTS_DIR --check
 *
 * WEIGHTS_DIR holds `weights_mobilenet_aesthetic_0.07.hdf5` and
 * `weights_mobilenet_technical_0.11.hdf5`, from
 * https://github.com/idealo/image-quality-assessment/tree/master/models/MobileNet
 *
 * The HDF5 reader (h5wasm) is fetched into a temporary folder the first time,
 * so the site itself does not depend on it.
 *
 * The format, little-endian:
 *   "NIMA", u32 version, u32 layer count, then for each layer
 *   u8 kind (0 conv 3x3, 1 depthwise 3x3, 2 pointwise, 3 dense), u8 stride,
 *   u16 reserved, u32 inputs, u32 outputs, the weights as f16 in Keras's own
 *   order, and one f32 bias per output.
 * Every BatchNorm is folded into the convolution before it.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const ROOT = resolve(import.meta.dirname, '..');
const OUT = join(ROOT, 'web/src/lib/wasm/critic');
const MODELS = { aesthetic: 'weights_mobilenet_aesthetic_0.07.hdf5',
                 technical: 'weights_mobilenet_technical_0.11.hdf5' };
const H5WASM = 'h5wasm@0.10.3';
const EPS = 1e-3;   // Keras's BatchNormalization default
const VERSION = 1;

// MobileNet v1 at alpha 1: the stride of each depthwise block.
const STRIDES = [1, 2, 1, 2, 1, 2, 1, 1, 1, 1, 1, 2, 1];

async function h5() {
  const dir = join(tmpdir(), 'lumiraw-h5wasm');
  const entry = join(dir, 'node_modules/h5wasm/dist/node/hdf5_hl.js');
  if (!existsSync(entry)) {
    mkdirSync(dir, { recursive: true });
    execFileSync('npm', ['install', '--no-audit', '--no-fund', '--prefix', dir, H5WASM],
                 { stdio: 'inherit' });
  }
  const mod = (await import(pathToFileURL(entry))).default;
  await mod.ready;
  return mod;
}

/** Fold a BatchNorm into the convolution before it: per output channel. */
function fold(kernel, outputs, bn) {
  const scale = new Float32Array(outputs);
  const bias = new Float32Array(outputs);
  for (let c = 0; c < outputs; c += 1) {
    scale[c] = bn.gamma[c] / Math.sqrt(bn.variance[c] + EPS);
    bias[c] = bn.beta[c] - bn.mean[c] * scale[c];
  }
  // Keras keeps the output channel last in every kernel shape used here.
  const weights = Float32Array.from(kernel, (w, i) => w * scale[i % outputs]);
  return { weights, bias };
}

function layers(file) {
  const get = (path) => Float32Array.from(file.get(path).value);
  const bn = (name) => ({
    gamma: get(`${name}/${name}/gamma:0`), beta: get(`${name}/${name}/beta:0`),
    mean: get(`${name}/${name}/moving_mean:0`), variance: get(`${name}/${name}/moving_variance:0`),
  });
  const out = [];
  out.push({ kind: 0, stride: 2, inputs: 3, outputs: 32,
             ...fold(get('conv1/conv1/kernel:0'), 32, bn('conv1_bn')) });
  let channels = 32;
  STRIDES.forEach((stride, i) => {
    const n = i + 1;
    out.push({ kind: 1, stride, inputs: channels, outputs: channels,
               ...fold(get(`conv_dw_${n}/conv_dw_${n}/depthwise_kernel:0`), channels,
                       bn(`conv_dw_${n}_bn`)) });
    const shape = file.get(`conv_pw_${n}/conv_pw_${n}/kernel:0`).shape;
    const outputs = shape[3];
    out.push({ kind: 2, stride: 1, inputs: channels, outputs,
               ...fold(get(`conv_pw_${n}/conv_pw_${n}/kernel:0`), outputs, bn(`conv_pw_${n}_bn`)) });
    channels = outputs;
  });
  out.push({ kind: 3, stride: 1, inputs: channels, outputs: 10,
             weights: get('dense_1/dense_1/kernel:0'), bias: get('dense_1/dense_1/bias:0') });
  return out;
}

function encode(list) {
  const size = 12 + list.reduce((s, l) => s + 12 + l.weights.length * 2 + l.bias.length * 4, 0);
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  new Uint8Array(buf).set([0x4e, 0x49, 0x4d, 0x41]);
  view.setUint32(4, VERSION, true);
  view.setUint32(8, list.length, true);
  let at = 12;
  for (const l of list) {
    view.setUint8(at, l.kind);
    view.setUint8(at + 1, l.stride);
    view.setUint32(at + 4, l.inputs, true);
    view.setUint32(at + 8, l.outputs, true);
    at += 12;
    for (const w of l.weights) { view.setFloat16(at, w, true); at += 2; }
    for (const b of l.bias) { view.setFloat32(at, b, true); at += 4; }
  }
  return new Uint8Array(buf);
}

/** The largest difference between what was folded and what the file holds. */
function compare(list, bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(8, true) !== list.length) throw new Error('layer count differs');
  let at = 12;
  let worst = 0;
  for (const l of list) {
    if (view.getUint8(at) !== l.kind || view.getUint32(at + 8, true) !== l.outputs) {
      throw new Error('layer header differs');
    }
    at += 12;
    for (const w of l.weights) {
      // f16 keeps 11 significant bits.
      const err = Math.abs(view.getFloat16(at, true) - w) / Math.max(Math.abs(w), 6e-5);
      worst = Math.max(worst, err);
      at += 2;
    }
    for (const b of l.bias) {
      worst = Math.max(worst, Math.abs(view.getFloat32(at, true) - b));
      at += 4;
    }
  }
  if (at !== bytes.length) throw new Error('trailing bytes');
  return worst;
}

const [dir, flag] = process.argv.slice(2);
if (!dir) {
  console.error('usage: node tools/bake-nima.mjs WEIGHTS_DIR [--check]');
  process.exit(2);
}
const lib = await h5();
mkdirSync(OUT, { recursive: true });
let failed = false;
for (const [id, name] of Object.entries(MODELS)) {
  const file = new lib.File(join(dir, name), 'r');
  const list = layers(file);
  file.close();
  const target = join(OUT, `nima-${id}.bin`);
  if (flag === '--check') {
    const worst = compare(list, readFileSync(target));
    const ok = worst < 1e-3;
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${id}: largest relative error ${worst.toExponential(2)}`);
    failed ||= !ok;
  } else {
    const bytes = encode(list);
    writeFileSync(target, bytes);
    console.log(`${target}  ${(bytes.length / 1e6).toFixed(1)} MB, ${list.length} layers`);
  }
}
process.exit(failed ? 1 : 0);
