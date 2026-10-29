#!/usr/bin/env node
/**
 * Bake MediaPipe's face models into the critic's graph format.
 *
 *   node tools/bake-faces.mjs <face_detection_full_range.tflite> <face_landmarks_detector.tflite>
 *
 * Both are Apache-2.0. The detector is
 * https://storage.googleapis.com/mediapipe-assets/face_detection_full_range.tflite,
 * and finds faces up to about five metres away. The landmark model is inside
 * https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task
 * (a zip), and places 478 points on a face, which is where the eyes are read
 * from; unlike the older face_landmark.tflite, its eyelids follow a blink.
 *
 * The output is `web/src/lib/wasm/critic/face-{detector,landmarks}.bin`:
 *
 *   "TFG1", u32 version
 *   u32 tensors, each: u8 rank, rank × i32 dims, u8 constant,
 *                      and if constant: u32 count, count × f16
 *   u32 operators, each: u8 kind, u8 inputs, u8 outputs, u8 params,
 *                        inputs × u32, outputs × u32, params × i32
 *   u32 input, u32 outputs, outputs × u32
 *
 * The weights are half floats in the models already and are copied as they
 * are, so nothing is lost: DEQUANTIZE (half to single) is folded away here,
 * once, rather than done at run time.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readTflite } from './tflite.mjs';

const KIND = { CONV_2D: 0, DEPTHWISE_CONV_2D: 1, ADD: 2, RELU: 3, PRELU: 4, PAD: 5,
               MAX_POOL_2D: 6, RESHAPE: 7, CONCATENATION: 8, RESIZE_BILINEAR: 9, LOGISTIC: 10 };
const ACT = { none: 0, relu: 1, relu6: 3 };

function bake(path) {
  const model = readTflite(readFileSync(path));
  // A DEQUANTIZE's output is its input's data: the half floats it would widen.
  const alias = new Map();
  for (const op of model.operators) {
    if (op.op === 'DEQUANTIZE') alias.set(op.outputs[0], model.tensors[op.inputs[0]]);
  }
  const ops = model.operators.filter((op) => op.op !== 'DEQUANTIZE');

  // Only the tensors something reads or writes, renumbered.
  const used = new Map();
  const id = (t) => {
    if (!used.has(t)) used.set(t, used.size);
    return used.get(t);
  };
  id(model.inputs[0]);
  const body = [];
  for (const op of ops) {
    const kind = KIND[op.op];
    if (kind == null) throw new Error(`${path}: ${op.op} is not supported`);
    const o = op.options;
    let inputs = op.inputs.filter((t) => t >= 0);
    let params = [];
    switch (op.op) {
      case 'CONV_2D':
      case 'DEPTHWISE_CONV_2D':
        if (o.dilation.some((d) => d !== 1)) throw new Error('dilated convolution');
        if (op.op === 'DEPTHWISE_CONV_2D' && o.multiplier !== 1) throw new Error('depth multiplier');
        params = [o.stride[1], o.stride[0], o.same, ACT[o.activation]];
        break;
      case 'ADD':
        params = [ACT[o.activation]];
        break;
      case 'PAD': {
        const { view, data } = model.tensors[op.inputs[1]];
        params = Array.from({ length: data.length / 4 }, (_, i) => view.getInt32(i * 4, true));
        inputs = inputs.slice(0, 1);
        break;
      }
      case 'MAX_POOL_2D':
        params = [o.filter[1], o.filter[0], o.stride[1], o.stride[0], o.same];
        break;
      case 'RESHAPE':
      case 'RESIZE_BILINEAR':
        // The shape comes from the output tensor.
        inputs = inputs.slice(0, 1);
        if (op.op === 'RESIZE_BILINEAR') params = [o.alignCorners ? 1 : 0, o.halfPixel ? 1 : 0];
        break;
      case 'CONCATENATION':
        params = [o.axis];
        break;
      default:
    }
    if (o.activation && o.activation !== 'none' && !['CONV_2D', 'DEPTHWISE_CONV_2D', 'ADD'].includes(op.op)) {
      throw new Error(`${op.op} with a fused ${op.options.activation}`);
    }
    body.push({ kind, inputs: inputs.map(id), outputs: op.outputs.map(id), params });
  }
  for (const t of model.outputs) id(t);

  const chunks = [];
  const u8 = (v) => chunks.push(Uint8Array.of(v));
  const u32 = (v) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); chunks.push(b); };
  const i32 = (v) => { const b = new Uint8Array(4); new DataView(b.buffer).setInt32(0, v, true); chunks.push(b); };
  chunks.push(new TextEncoder().encode('TFG1'));
  u32(1);
  u32(used.size);
  let weights = 0;
  for (const [original] of [...used.entries()].sort((a, b) => a[1] - b[1])) {
    const t = model.tensors[original];
    const source = alias.get(original) ?? t;
    const shape = t.shape.length ? t.shape : [1];
    u8(shape.length);
    shape.forEach(i32);
    // Half floats as they are; single ones narrowed, rounded to nearest.
    const width = { f16: 2, f32: 4 }[source.type];
    const count = source.data && width ? source.data.length / width : 0;
    u8(count ? 1 : 0);
    if (count) {
      u32(count);
      if (width === 2) {
        chunks.push(source.data);
      } else {
        const half = new DataView(new ArrayBuffer(count * 2));
        for (let i = 0; i < count; i += 1) half.setFloat16(i * 2, source.view.getFloat32(i * 4, true), true);
        chunks.push(new Uint8Array(half.buffer));
      }
      weights += count;
    }
  }
  u32(body.length);
  for (const op of body) {
    u8(op.kind); u8(op.inputs.length); u8(op.outputs.length); u8(op.params.length);
    op.inputs.forEach(u32);
    op.outputs.forEach(u32);
    op.params.forEach(i32);
  }
  u32(id(model.inputs[0]));
  u32(model.outputs.length);
  model.outputs.forEach((t) => u32(id(t)));
  const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let at = 0;
  for (const c of chunks) { out.set(c, at); at += c.length; }
  return { bytes: out, ops: body.length, tensors: used.size, weights };
}

const [detector, landmarks] = process.argv.slice(2);
if (!detector || !landmarks) {
  console.error('usage: node tools/bake-faces.mjs <face_detection_full_range.tflite> <face_landmarks_detector.tflite>');
  process.exit(1);
}
const dir = join(dirname(fileURLToPath(import.meta.url)), '../web/src/lib/wasm/critic');
for (const [name, path] of [['face-detector', detector], ['face-landmarks', landmarks]]) {
  const { bytes, ops, tensors, weights } = bake(path);
  writeFileSync(join(dir, `${name}.bin`), bytes);
  console.log(`${name}.bin: ${ops} operators, ${tensors} tensors, ${weights} weights, ${bytes.length} bytes`);
}
