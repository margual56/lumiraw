/** Just enough of a TFLite reader for the baking scripts: the model's
 *  flatbuffer (schema v3) read into plain objects, without dependencies. */

/** The operators the face models use; anything else comes out as `op<n>`. */
const OPS = {
  0: 'ADD', 2: 'CONCATENATION', 3: 'CONV_2D', 4: 'DEPTHWISE_CONV_2D', 6: 'DEQUANTIZE',
  14: 'LOGISTIC', 17: 'MAX_POOL_2D', 19: 'RELU', 22: 'RESHAPE', 23: 'RESIZE_BILINEAR', 34: 'PAD',
  54: 'PRELU',
};
const TYPES = { 0: 'f32', 1: 'f16', 2: 'i32', 3: 'u8', 4: 'i64', 9: 'i8' };

class Table {
  constructor(view, pos) {
    this.view = view;
    this.pos = pos;
    const vt = pos - view.getInt32(pos, true);
    this.vt = vt;
    this.vtLen = view.getUint16(vt, true);
  }
  offset(slot) {
    const o = 4 + 2 * slot;
    return o < this.vtLen ? this.view.getUint16(this.vt + o, true) : 0;
  }
  scalar(slot, kind, fallback = 0) {
    const o = this.offset(slot);
    if (!o) return fallback;
    const at = this.pos + o;
    switch (kind) {
      case 'u8': return this.view.getUint8(at);
      case 'i8': return this.view.getInt8(at);
      case 'i32': return this.view.getInt32(at, true);
      case 'u32': return this.view.getUint32(at, true);
      case 'u64': return Number(this.view.getBigUint64(at, true));
      default: throw new Error(kind);
    }
  }
  indirect(slot) {
    const o = this.offset(slot);
    if (!o) return null;
    const at = this.pos + o;
    return at + this.view.getUint32(at, true);
  }
  table(slot) {
    const at = this.indirect(slot);
    return at == null ? null : new Table(this.view, at);
  }
  vector(slot, kind) {
    const at = this.indirect(slot);
    if (at == null) return [];
    const len = this.view.getUint32(at, true);
    const start = at + 4;
    const out = [];
    for (let i = 0; i < len; i += 1) {
      if (kind === 'table') {
        const p = start + 4 * i;
        out.push(new Table(this.view, p + this.view.getUint32(p, true)));
      } else if (kind === 'i32') out.push(this.view.getInt32(start + 4 * i, true));
      else if (kind === 'bytes') return new Uint8Array(this.view.buffer, this.view.byteOffset + start, len);
    }
    return out;
  }
}

/** Options tables as plain objects, for the operators the face models use.
 *  `same` is TensorFlow's padding flag, 1 for "same" and 0 for "valid". */
function options(op, t) {
  if (!t) return {};
  const act = (slot) => ['none', 'relu', 'relu_n1_1', 'relu6', 'tanh'][t.scalar(slot, 'i8')] ?? 'other';
  const same = t.scalar(0, 'i8') ? 0 : 1;
  switch (op) {
    case 'CONV_2D':
      return { same, stride: [t.scalar(1, 'i32'), t.scalar(2, 'i32')], activation: act(3),
               dilation: [t.scalar(4, 'i32', 1), t.scalar(5, 'i32', 1)] };
    case 'DEPTHWISE_CONV_2D':
      return { same, stride: [t.scalar(1, 'i32'), t.scalar(2, 'i32')], multiplier: t.scalar(3, 'i32'),
               activation: act(4), dilation: [t.scalar(5, 'i32', 1), t.scalar(6, 'i32', 1)] };
    case 'MAX_POOL_2D':
      return { same, stride: [t.scalar(1, 'i32'), t.scalar(2, 'i32')],
               filter: [t.scalar(3, 'i32'), t.scalar(4, 'i32')], activation: act(5) };
    case 'ADD':
      return { activation: act(0) };
    case 'CONCATENATION':
      return { axis: t.scalar(0, 'i32'), activation: act(1) };
    case 'RESIZE_BILINEAR':
      return { alignCorners: Boolean(t.scalar(2, 'u8')), halfPixel: Boolean(t.scalar(3, 'u8')) };
    default:
      return {};
  }
}

export function readTflite(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const model = new Table(view, view.getUint32(0, true));
  const codes = model.vector(1, 'table').map((c) => {
    const code = Math.max(c.scalar(0, 'i8'), c.scalar(3, 'i32'));
    return OPS[code] ?? `op${code}`;
  });
  const buffers = model.vector(4, 'table').map((b) => {
    const data = b.vector(0, 'bytes');
    if (data.length) return data;
    const offset = b.scalar(1, 'u64');
    const size = b.scalar(2, 'u64');
    return size ? bytes.subarray(offset, offset + size) : new Uint8Array(0);
  });
  const graph = model.vector(2, 'table')[0];
  // Constants as their raw little-endian bytes, plus a DataView to read them.
  const tensors = graph.vector(0, 'table').map((t) => {
    const raw = buffers[t.scalar(2, 'u32')];
    const data = raw?.length ? raw.slice() : null;
    return { shape: t.vector(0, 'i32'), type: TYPES[t.scalar(1, 'i8')] ?? `t${t.scalar(1, 'i8')}`,
             data, view: data && new DataView(data.buffer) };
  });
  const operators = graph.vector(3, 'table').map((o) => {
    const op = codes[o.scalar(0, 'u32')];
    return { op, inputs: o.vector(1, 'i32'), outputs: o.vector(2, 'i32'), options: options(op, o.table(4)) };
  });
  return { tensors, operators, inputs: graph.vector(1, 'i32'), outputs: graph.vector(2, 'i32') };
}
