#!/usr/bin/env node
/**
 * Bake PMRID, the raw denoiser, into the critic's graph format.
 *
 *   node tools/bake-denoiser.mjs <torch_pretrained.ckp>
 *
 * PMRID is Megvii's network from "Practical Deep Raw Image Denoising on Mobile
 * Devices" (Wang et al., ECCV 2020), Apache-2.0, at
 * https://github.com/megvii-research/PMRID (`models/torch_pretrained.ckp`). It
 * cleans a Bayer mosaic packed into four half-size channels (R, G, G, B), a
 * U-Net of separable 5 × 5 convolutions small enough to run on a phone.
 *
 * The checkpoint is a PyTorch state dict: a zip, stored rather than deflated,
 * holding a pickle that names each tensor and where its numbers are. Both are
 * read here with just enough of each format. The network is laid out by hand
 * after `models/net_torch.py`, for one fixed tile (`TILE` packed pixels a
 * side); the pipeline cuts a frame into tiles of that size.
 *
 * The output is `web/src/lib/wasm/critic/denoiser.bin`, in the format
 * `tools/tfg.mjs` describes, the weights narrowed to half floats.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ACT, KIND, toHalf, writeGraph } from './tfg.mjs';

/** Packed pixels a side of the tile the graph is laid out for. */
export const TILE = 256;

// --- the zip ---------------------------------------------------------------------

/** Every member of a zip whose members are stored, by name. */
function unzip(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let end = bytes.length - 22;
  while (end >= 0 && view.getUint32(end, true) !== 0x06054b50) end -= 1;
  if (end < 0) throw new Error('not a zip');
  const count = view.getUint16(end + 10, true);
  let at = view.getUint32(end + 16, true);
  const files = new Map();
  for (let i = 0; i < count; i += 1) {
    if (view.getUint32(at, true) !== 0x02014b50) throw new Error('a broken zip directory');
    const method = view.getUint16(at + 10, true);
    const size = view.getUint32(at + 20, true);
    const nameLen = view.getUint16(at + 28, true);
    const extraLen = view.getUint16(at + 30, true);
    const commentLen = view.getUint16(at + 32, true);
    const local = view.getUint32(at + 42, true);
    const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLen));
    if (method !== 0) throw new Error(`${name} is compressed`);
    const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    files.set(name, bytes.subarray(start, start + size));
    at += 46 + nameLen + extraLen + commentLen;
  }
  return files;
}

// --- the pickle -------------------------------------------------------------------

/** A PyTorch state dict's pickle, run far enough to give each tensor's storage
 *  key, offset and shape: `Map(name -> { key, offset, shape })`. */
function unpickle(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const text = (at, n) => new TextDecoder().decode(bytes.subarray(at, at + n));
  const stack = [];
  const marks = [];
  const memo = new Map();
  const MARK = Symbol('mark');
  const popMark = () => {
    const at = marks.pop();
    return stack.splice(at);
  };
  const call = (fn, args) => {
    if (fn === 'collections OrderedDict') return new Map();
    if (fn === 'torch._utils _rebuild_tensor_v2') {
      const [storage, offset, shape] = args;
      return { key: storage.key, offset, shape };
    }
    return { fn, args };
  };
  let at = 0;
  for (;;) {
    const op = bytes[at++];
    switch (op) {
      case 0x80: at += 1; break;                                   // PROTO
      case 0x63: {                                                 // GLOBAL
        const nl1 = bytes.indexOf(10, at);
        const nl2 = bytes.indexOf(10, nl1 + 1);
        stack.push(`${text(at, nl1 - at)} ${text(nl1 + 1, nl2 - nl1 - 1)}`);
        at = nl2 + 1;
        break;
      }
      case 0x71: memo.set(bytes[at++], stack.at(-1)); break;       // BINPUT
      case 0x72: memo.set(view.getUint32(at, true), stack.at(-1)); at += 4; break;
      case 0x68: stack.push(memo.get(bytes[at++])); break;        // BINGET
      case 0x6a: stack.push(memo.get(view.getUint32(at, true))); at += 4; break;
      case 0x29: stack.push([]); break;                           // EMPTY_TUPLE
      case 0x7d: stack.push(new Map()); break;                    // EMPTY_DICT
      case 0x5d: stack.push([]); break;                           // EMPTY_LIST
      case 0x28: marks.push(stack.length); stack.push(MARK); break;
      case 0x74: { const items = popMark(); items.shift(); stack.push(items); break; }
      case 0x85: stack.push([stack.pop()]); break;                // TUPLE1
      case 0x86: { const b = stack.pop(); stack.push([stack.pop(), b]); break; }
      case 0x87: { const c = stack.pop(); const b = stack.pop(); stack.push([stack.pop(), b, c]); break; }
      case 0x58: {                                                 // BINUNICODE
        const n = view.getUint32(at, true);
        stack.push(text(at + 4, n));
        at += 4 + n;
        break;
      }
      case 0x8c: { const n = bytes[at]; stack.push(text(at + 1, n)); at += 1 + n; break; }
      case 0x4b: stack.push(bytes[at++]); break;                   // BININT1
      case 0x4d: stack.push(view.getUint16(at, true)); at += 2; break;
      case 0x4a: stack.push(view.getInt32(at, true)); at += 4; break;
      case 0x8a: {                                                 // LONG1
        const n = bytes[at];
        let v = 0;
        for (let i = n - 1; i >= 0; i -= 1) v = v * 256 + bytes[at + 1 + i];
        stack.push(v);
        at += 1 + n;
        break;
      }
      case 0x47: stack.push(view.getFloat64(at, false)); at += 8; break;
      case 0x88: stack.push(true); break;
      case 0x89: stack.push(false); break;
      case 0x4e: stack.push(null); break;
      case 0x51: {                                                 // BINPERSID
        const [, , key] = stack.pop();
        stack.push({ key });
        break;
      }
      case 0x52: { const args = stack.pop(); const fn = stack.pop(); stack.push(call(fn, args)); break; }
      case 0x62: stack.pop(); break;                               // BUILD: state ignored
      case 0x73: { const v = stack.pop(); const k = stack.pop(); stack.at(-1).set(k, v); break; }
      case 0x75: {                                                 // SETITEMS
        const items = popMark();
        items.shift();
        const dict = stack.at(-1);
        for (let i = 0; i < items.length; i += 2) dict.set(items[i], items[i + 1]);
        break;
      }
      case 0x61: { const v = stack.pop(); stack.at(-1).push(v); break; }
      case 0x65: { const items = popMark(); items.shift(); stack.at(-1).push(...items); break; }
      case 0x2e: return stack.pop();                               // STOP
      default: throw new Error(`pickle opcode 0x${op.toString(16)} at ${at - 1}`);
    }
  }
}

/** The checkpoint's tensors, by name, as Float32Arrays with their shapes. */
function weights(path) {
  const files = unzip(readFileSync(path));
  const prefix = [...files.keys()].find((k) => k.endsWith('data.pkl')).replace('data.pkl', '');
  const dict = unpickle(files.get(`${prefix}data.pkl`));
  const out = new Map();
  for (const [name, { key, offset, shape }] of dict) {
    const raw = files.get(`${prefix}data/${key}`);
    const all = new Float32Array(raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength));
    const n = shape.reduce((a, b) => a * b, 1);
    out.set(name, { shape, data: all.subarray(offset, offset + n) });
  }
  return out;
}

// --- the network ------------------------------------------------------------------

function build(w) {
  const tensors = [];
  const ops = [];
  let macs = 0;
  const tensor = (shape, values = null) => {
    tensors.push({ shape, half: values ? toHalf(values) : null });
    return { id: tensors.length - 1, shape };
  };
  const take = (name) => {
    const t = w.get(name);
    if (!t) throw new Error(`no ${name} in the checkpoint`);
    w.delete(name);
    return t;
  };
  const op = (kind, inputs, shape, params) => {
    const out = tensor(shape);
    ops.push({ kind, inputs: inputs.map((t) => t.id), outputs: [out.id], params });
    return out;
  };
  /** PyTorch pads evenly; TensorFlow's "same" at stride two does not, so a
   *  strided layer gets its padding as an operator and pads nothing itself. */
  const padded = (x, p) => op(KIND.PAD, [x], [1, x.shape[1] + 2 * p, x.shape[2] + 2 * p, x.shape[3]],
                              [0, 0, p, p, p, p, 0, 0]);
  const outSize = (x, k, stride) => (stride === 1 ? x.shape[1] : Math.floor((x.shape[1] + (k - 1) - k) / stride) + 1);

  /** A full convolution, torch weight [out, in, kh, kw]. */
  function conv(x, name, stride, act) {
    const { shape: [o, i, kh, kw], data } = take(`${name}.weight`);
    const bias = take(`${name}.bias`).data;
    const f = new Float32Array(o * kh * kw * i);
    for (let a = 0; a < o; a++) for (let y = 0; y < kh; y++) for (let z = 0; z < kw; z++) {
      for (let b = 0; b < i; b++) f[((a * kh + y) * kw + z) * i + b] = data[((a * i + b) * kh + y) * kw + z];
    }
    const side = outSize(x, kh, stride);
    const input = stride === 1 ? x : padded(x, (kh - 1) / 2);
    macs += side * side * o * i * kh * kw;
    return op(KIND.CONV_2D, [input, tensor([o, kh, kw, i], f), tensor([o], bias)], [1, side, side, o],
              [stride, stride, stride === 1 ? 1 : 0, ACT[act]]);
  }
  /** Depthwise then pointwise, torch's `is_seperable`. */
  function separable(x, name, stride, act) {
    const { shape: [c, , kh, kw], data } = take(`${name}.depthwise.weight`);
    const f = new Float32Array(kh * kw * c);
    for (let a = 0; a < c; a++) for (let k = 0; k < kh * kw; k++) f[k * c + a] = data[a * kh * kw + k];
    const side = outSize(x, kh, stride);
    const input = stride === 1 ? x : padded(x, (kh - 1) / 2);
    macs += side * side * c * kh * kw;
    const dw = op(KIND.DEPTHWISE_CONV_2D, [input, tensor([1, kh, kw, c], f)], [1, side, side, c],
                  [stride, stride, stride === 1 ? 1 : 0, ACT.none]);
    return conv(dw, `${name}.pointwise`, 1, act);
  }
  const add = (a, b, act = 'none') => op(KIND.ADD, [a, b], a.shape, [ACT[act]]);

  function encoderBlock(x, name, stride) {
    const proj = w.has(`${name}.proj.depthwise.weight`) ? separable(x, `${name}.proj`, stride, 'none') : x;
    const y = separable(separable(x, `${name}.conv1`, stride, 'relu'), `${name}.conv2`, 1, 'none');
    return add(y, proj, 'relu');
  }
  function encoderStage(x, name, blocks) {
    for (let b = 0; b < blocks; b++) x = encoderBlock(x, `${name}.${b}`, b === 0 ? 2 : 1);
    return x;
  }
  function decoderBlock(x, name) {
    return add(separable(separable(x, `${name}.conv0`, 1, 'relu'), `${name}.conv1`, 1, 'none'), x);
  }
  /** ConvTranspose 2 × 2, stride 2, torch weight [in, out, 2, 2]: a 1 × 1
   *  convolution to four times the channels, then depth to space. */
  function upsample(x, name) {
    const { shape: [i, o], data } = take(`${name}.weight`);
    const bias = take(`${name}.bias`).data;
    const f = new Float32Array(4 * o * i);
    const b4 = new Float32Array(4 * o);
    for (let d = 0; d < 4; d++) for (let a = 0; a < o; a++) {
      b4[d * o + a] = bias[a];
      for (let c = 0; c < i; c++) f[(d * o + a) * i + c] = data[(c * o + a) * 4 + d];
    }
    const side = x.shape[1];
    macs += side * side * 4 * o * i;
    const wide = op(KIND.CONV_2D, [x, tensor([4 * o, 1, 1, i], f), tensor([4 * o], b4)],
                    [1, side, side, 4 * o], [1, 1, 1, ACT.none]);
    return op(KIND.DEPTH_TO_SPACE, [wide], [1, side * 2, side * 2, o], [2]);
  }
  function decoderStage(x, skip, name) {
    const up = upsample(decoderBlock(x, `${name}.decode_conv`), `${name}.upsample`);
    return add(up, separable(skip, `${name}.proj_conv`, 1, 'relu'));
  }

  const input = tensor([1, TILE, TILE, 4]);
  const c0 = conv(input, 'conv0.conv', 1, 'relu');
  const c1 = encoderStage(c0, 'enc1', 2);
  const c2 = encoderStage(c1, 'enc2', 2);
  const c3 = encoderStage(c2, 'enc3', 4);
  const c4 = encoderStage(c3, 'enc4', 4);
  const c5 = separable(c4, 'encdec', 1, 'relu');
  const u3 = decoderStage(c5, c3, 'dec1');
  const u2 = decoderStage(u3, c2, 'dec2');
  const u1 = decoderStage(u2, c1, 'dec3');
  const u0 = decoderStage(u1, c0, 'dec4');
  const residual = conv(decoderBlock(u0, 'out0'), 'out1.conv', 1, 'none');
  const output = add(input, residual);
  if (w.size) throw new Error(`weights left over: ${[...w.keys()].join(', ')}`);
  return { graph: { tensors, ops, input: input.id, outputs: [output.id] }, macs };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [path] = process.argv.slice(2);
  if (!path) {
    console.error('usage: node tools/bake-denoiser.mjs <torch_pretrained.ckp>');
    process.exit(1);
  }
  const { graph, macs } = build(weights(path));
  const { bytes, ops, tensors, weights: count } = writeGraph(graph);
  const out = join(dirname(fileURLToPath(import.meta.url)), '../web/src/lib/wasm/critic/denoiser.bin');
  writeFileSync(out, bytes);
  console.log(`denoiser.bin: ${ops} operators, ${tensors} tensors, ${count} weights, ${bytes.length} bytes,`
              + ` ${(macs / (TILE * TILE * 4)).toFixed(0)} multiply-adds a raw pixel`);
}
