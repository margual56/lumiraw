/** The graph format the critic's executor reads (`core/critic/src/graph.rs`),
 *  written from a TFLite model or from a network laid out by hand:
 *
 *   "TFG1", u32 version
 *   u32 tensors, each: u8 rank, rank × i32 dims, u8 constant,
 *                      and if constant: u32 count, count × f16
 *   u32 operators, each: u8 kind, u8 inputs, u8 outputs, u8 params,
 *                        inputs × u32, outputs × u32, params × i32
 *   u32 input, u32 outputs, outputs × u32
 */
import { readFileSync } from 'node:fs';
import { readTflite } from './tflite.mjs';

export const KIND = { CONV_2D: 0, DEPTHWISE_CONV_2D: 1, ADD: 2, RELU: 3, PRELU: 4, PAD: 5,
                      MAX_POOL_2D: 6, RESHAPE: 7, CONCATENATION: 8, RESIZE_BILINEAR: 9, LOGISTIC: 10,
                      AVERAGE_POOL_2D: 11, DEPTH_TO_SPACE: 12 };
export const ACT = { none: 0, relu: 1, relu6: 3 };

/** Values as half floats, rounded to nearest. */
export function toHalf(values) {
  const half = new DataView(new ArrayBuffer(values.length * 2));
  for (let i = 0; i < values.length; i += 1) half.setFloat16(i * 2, values[i], true);
  return new Uint8Array(half.buffer);
}

/** A TFLite model as a graph: `{ tensors: [{ shape, half }], ops, input, outputs }`,
 *  `half` being a constant's values as f16 bytes, or null. */
export function fromTflite(path) {
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
        if (op.op === 'DEPTHWISE_CONV_2D' && o.multiplier !== 1) throw new Error('depth multiplier');
        params = [o.stride[1], o.stride[0], o.same, ACT[o.activation]];
        // Dilation only when there is some, so a model without any bakes as it
        // always did.
        if (o.dilation.some((d) => d !== 1)) params.push(o.dilation[1], o.dilation[0]);
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
      case 'AVERAGE_POOL_2D':
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
      case 'DEPTH_TO_SPACE':
        params = [o.block];
        break;
      default:
    }
    if (o.activation && o.activation !== 'none'
        && !['CONV_2D', 'DEPTHWISE_CONV_2D', 'ADD'].includes(op.op)) {
      throw new Error(`${op.op} with a fused ${op.options.activation}`);
    }
    body.push({ kind, inputs: inputs.map(id), outputs: op.outputs.map(id), params });
  }
  for (const t of model.outputs) id(t);

  const tensors = [...used.entries()].sort((a, b) => a[1] - b[1]).map(([original]) => {
    const t = model.tensors[original];
    const source = alias.get(original) ?? t;
    const shape = t.shape.length ? t.shape : [1];
    // Half floats as they are; single ones narrowed.
    let half = null;
    if (source.data && source.type === 'f16') half = source.data;
    else if (source.data && source.type === 'f32') {
      const count = source.data.length / 4;
      half = toHalf(Array.from({ length: count }, (_, i) => source.view.getFloat32(i * 4, true)));
    }
    return { shape, half };
  });
  return { tensors, ops: body, input: id(model.inputs[0]), outputs: model.outputs.map(id) };
}

/** A graph in the file format, with a count of what went in. */
export function writeGraph({ tensors, ops, input, outputs }) {
  const chunks = [];
  const u8 = (v) => chunks.push(Uint8Array.of(v));
  const u32 = (v) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); chunks.push(b); };
  const i32 = (v) => { const b = new Uint8Array(4); new DataView(b.buffer).setInt32(0, v, true); chunks.push(b); };
  chunks.push(new TextEncoder().encode('TFG1'));
  u32(1);
  u32(tensors.length);
  let weights = 0;
  for (const { shape, half } of tensors) {
    u8(shape.length);
    shape.forEach(i32);
    const count = half ? half.length / 2 : 0;
    u8(count ? 1 : 0);
    if (count) {
      u32(count);
      chunks.push(half);
      weights += count;
    }
  }
  u32(ops.length);
  for (const op of ops) {
    u8(op.kind); u8(op.inputs.length); u8(op.outputs.length); u8(op.params.length);
    op.inputs.forEach(u32);
    op.outputs.forEach(u32);
    op.params.forEach(i32);
  }
  u32(input);
  u32(outputs.length);
  outputs.forEach(u32);
  const bytes = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let at = 0;
  for (const c of chunks) { bytes.set(c, at); at += c.length; }
  return { bytes, ops: ops.length, tensors: tensors.length, weights };
}
