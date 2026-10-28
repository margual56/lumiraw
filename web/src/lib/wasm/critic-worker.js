/** NIMA scoring, off the main thread. The page runs a few of these at once;
 *  each is handed the compiled module and the weights rather than fetching
 *  them itself, so they are downloaded once however many there are. */

let wasm = null;
let memory = null;
const decoder = new TextDecoder();

// Slots in the module (see core/critic/src/wasm.rs).
const AESTHETIC = 0;
const TECHNICAL = 1;

function text() {
  return decoder.decode(new Uint8Array(memory.buffer, wasm.cr_text_ptr(), wasm.cr_text_len()));
}

function withBytes(bytes, fn) {
  const ptr = wasm.cr_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  try {
    return fn(ptr, bytes.length);
  } finally {
    wasm.cr_free(ptr, bytes.length);
  }
}

async function init({ module, aesthetic, technical }) {
  const instance = await WebAssembly.instantiate(module, {});
  wasm = instance.exports;
  memory = wasm.memory;
  for (const [slot, buffer] of [[AESTHETIC, aesthetic], [TECHNICAL, technical]]) {
    const rc = withBytes(new Uint8Array(buffer), (p, n) => wasm.cr_load(slot, p, n));
    if (rc !== 0) throw new Error(text());
  }
  return {};
}

/** Both networks on one 224 by 224 RGBA picture. */
function score({ pixels, width, height }) {
  return withBytes(new Uint8Array(pixels), (ptr) => {
    const run = (slot) => {
      if (wasm.cr_score(slot, ptr, width, height) !== 0) throw new Error(text());
      // Copied out: the next call may move the module's memory.
      return new Float32Array(memory.buffer, wasm.cr_out_ptr(), wasm.cr_out_len()).slice();
    };
    const a = run(AESTHETIC);
    const t = run(TECHNICAL);
    // The aesthetic network's features: it was trained on what pictures are
    // of, which is what tells two frames of a burst from two shots.
    const feature = a.slice(2);
    return { aesthetic: a[0], aesthetic_std: a[1], technical: t[0], technical_std: t[1],
             feature, transfer: [feature.buffer] };
  });
}

const HANDLERS = { init, score };

onmessage = async ({ data: { type, id, ...rest } }) => {
  try {
    const out = await HANDLERS[type](rest);
    const { transfer = [], ...data } = out;
    postMessage({ id, ok: true, data }, transfer);
  } catch (error) {
    postMessage({ id, ok: false, error: error.message });
  }
};
