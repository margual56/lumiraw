/** The pipeline, off the main thread. */

let wasm = null;          // exports
let memory = null;
let ready = null;         // the init promise
let current = null;       // id of the call in flight, for progress messages
let queue = Promise.resolve();   // one request at a time: one wasm session

import { stamp } from './stamp.js';
import { withExif } from './webp.js';

// Imported rather than fetched from a fixed path so the bundler fingerprints
// them.
import wasmUrl from './autoraw_core.wasm?url';
import databaseUrl from './lensfun.json?url';

const decoder = new TextDecoder();
const encoder = new TextEncoder();

/** wasm calls back into here at every stage boundary. */
function hostProgress(fraction, ptr, len) {
  const code = decoder.decode(new Uint8Array(memory.buffer, ptr, len));
  if (current !== null) postMessage({ type: 'progress', id: current, fraction, code });
}

/** Streaming compilation needs the server to say `application/wasm`; not every
 *  host does, so fall back to compiling from the bytes rather than failing. */
async function compile(url, imports) {
  try {
    return await WebAssembly.instantiateStreaming(fetch(url), imports);
  } catch {
    const bytes = await (await fetch(url)).arrayBuffer();
    return WebAssembly.instantiate(bytes, imports);
  }
}

async function init() {
  const [module, db] = await Promise.all([
    compile(wasmUrl, { env: { host_progress: hostProgress } }),
    fetch(databaseUrl).then((r) => (r.ok ? r.arrayBuffer() : null)).catch(() => null),
  ]);
  wasm = module.instance.exports;
  memory = wasm.memory;
  wasm.ar_version();
  const { version } = readJson();
  if (db) {
    const ptr = copyIn(new Uint8Array(db));
    wasm.ar_set_database(ptr, db.byteLength);
    wasm.ar_free(ptr, db.byteLength);
  }
  return { version, lenses: db ? readJson().lenses : 0 };
}

/** Copy bytes into wasm memory and return the pointer (freed by the caller). */
function copyIn(bytes) {
  const ptr = wasm.ar_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  return ptr;
}

/** Hand a block of bytes to the module for the duration of one call. */
function withBytes(bytes, fn) {
  const ptr = copyIn(bytes);
  try {
    return fn(ptr, bytes.length);
  } finally {
    wasm.ar_free(ptr, bytes.length);
  }
}

function withString(text, fn) {
  return withBytes(encoder.encode(text), fn);
}

function readJson() {
  const ptr = wasm.ar_json_ptr();
  const len = wasm.ar_json_len();
  if (!len) return {};
  return JSON.parse(decoder.decode(new Uint8Array(memory.buffer, ptr, len)));
}

function fail(info) {
  const error = new Error(info.error || 'the pipeline failed');
  error.reason = info.code;
  throw error;
}

// -- pixels -> something an <img> can show --------------------------------

/** WebP at 0.92 is a good preview carrier: encoding a 1300px frame costs a few
 *  milliseconds and the blob is a fraction of a PNG's. */
async function toBlob(pixels, w, h, type = 'image/webp', quality = 0.92) {
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext('2d');
  ctx.putImageData(new ImageData(new Uint8ClampedArray(pixels), w, h), 0, 0);
  return canvas.convertToBlob({ type, quality });
}

function primary() {
  const w = wasm.ar_width();
  const h = wasm.ar_height();
  const ptr = wasm.ar_pixels_ptr();
  const len = wasm.ar_pixels_len();
  // Copy out: the next call into wasm may grow (and therefore move) memory.
  return { w, h, pixels: new Uint8Array(memory.buffer, ptr, len).slice() };
}

function secondary() {
  const w = wasm.ar_width_b();
  const h = wasm.ar_height_b();
  const ptr = wasm.ar_pixels_b_ptr();
  const len = wasm.ar_pixels_b_len();
  return { w, h, pixels: new Uint8Array(memory.buffer, ptr, len).slice() };
}

function encodedBytes() {
  const ptr = wasm.ar_bytes_ptr();
  const len = wasm.ar_bytes_len();
  return new Uint8Array(memory.buffer, ptr, len).slice();
}

// -- operations ------------------------------------------------------------

async function open({ name, buffer }) {
  wasm.ar_close();
  const bytes = new Uint8Array(buffer);
  const ptr = copyIn(bytes);
  const rc = withString(name, (np, nl) => wasm.ar_open(np, nl, ptr, bytes.length));
  wasm.ar_free(ptr, bytes.length);
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

function settingsPointer(settings, fn) {
  return withString(JSON.stringify(settings ?? {}), fn);
}

async function render({ settings, size, uncropped }) {
  const rc = settingsPointer(settings, (sp, sl) =>
    wasm.ar_render(sp, sl, size ?? 1300, uncropped ? 0 : 1));
  const info = readJson();
  if (rc !== 0) fail(info);
  const { w, h, pixels } = primary();
  const blob = await toBlob(pixels, w, h);
  return {
    image: blob,
    width: w,
    height: h,
    toggles: info.toggles ?? [],
    report: info.report ?? {},
    cover_crop: info.crop ?? null,
  };
}

async function compare({ settings, size }) {
  const rc = settingsPointer(settings, (sp, sl) => wasm.ar_compare(sp, sl, size ?? 1300));
  const info = readJson();
  if (rc !== 0) fail(info);
  const after = primary();
  const before = secondary();
  return {
    after: await toBlob(after.pixels, after.w, after.h),
    before: await toBlob(before.pixels, before.w, before.h),
    toggles: info.toggles ?? [],
    report: info.report ?? {},
  };
}

/** Every named grade, with the control points that define it. */
async function looks() {
  if (wasm.ar_looks() !== 0) fail(readJson());
  return readJson();
}

/** The four curves a settings object comes to, baked, for drawing. */
async function curves({ settings }) {
  const rc = settingsPointer(settings, (sp, sl) => wasm.ar_curves(sp, sl));
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

/** Read a `.cube` file into the module, where it stays. */
async function lutLoad({ buffer }) {
  const rc = withBytes(new Uint8Array(buffer), (p, n) => wasm.ar_lut_load(p, n));
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

function lutClear() {
  wasm.ar_lut_clear();
  return readJson();
}

async function exportImage({ settings, format, quality, max_size, original_name }) {
  // There is no clock in the wasm, so the host passes the time in.
  const now = stamp();
  const stem = (original_name || 'photo').replace(/\.[^.]+$/, '');
  const rc = settingsPointer(settings, (sp, sl) =>
    withString(format, (fp, fl) =>
      withString(now, (np, nl) =>
        wasm.ar_export(sp, sl, fp, fl, quality ?? 92,
                       max_size ? Number(max_size) : 0, np, nl))));
  const info = readJson();
  if (rc !== 0) fail(info);
  let blob;
  if (info.canvas) {
    // WebP has no pure-Rust encoder worth carrying, so the browser's own
    // encoder finishes the job, and then the metadata is spliced back in,
    // because the canvas throws all of it away.
    const { w, h, pixels } = primary();
    blob = await toBlob(pixels, w, h, 'image/webp', (quality ?? 92) / 100);
    if (info.exif) {
      blob = await withExif(blob, encodedBytes(), w, h);
    }
  } else {
    blob = new Blob([encodedBytes()], { type: info.mime });
  }
  return { blob, filename: `${stem}.${info.ext ?? 'webp'}` };
}

async function mergeAdd({ name, buffer }) {
  const bytes = new Uint8Array(buffer);
  const ptr = copyIn(bytes);
  const rc = withString(name, (np, nl) => wasm.ar_merge_add(np, nl, ptr, bytes.length));
  wasm.ar_free(ptr, bytes.length);
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

/** What is odd about the bracket so far, from the metadata alone. */
function mergeCheck() {
  wasm.ar_merge_check();
  return readJson();
}

function mergeRemove({ index }) {
  wasm.ar_merge_remove(index);
  return {};
}

function mergeReset() {
  wasm.ar_merge_reset();
  return {};
}

/** Merge the bracket; the result becomes the frame the wizard develops. */
async function mergeFinish({ align, deghost }) {
  // A negative amount asks the merge to measure one, which is the default.
  const rc = wasm.ar_merge_finish(align ? 1 : 0, deghost ?? -1);
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

const HANDLERS = { open, render, compare, looks, curves, lutLoad, lutClear,
                   export: exportImage,
                   mergeAdd, mergeRemove, mergeReset, mergeFinish, mergeCheck };

async function handle({ type, id, ...rest }) {
  try {
    ready = ready || init();
    const info = await ready;
    if (type === 'init') {
      postMessage({ type: 'result', id, ok: true, data: info });
      return;
    }
    const handler = HANDLERS[type];
    if (!handler) throw new Error(`unknown request: ${type}`);
    current = id;
    const data = await handler(rest);
    postMessage({ type: 'result', id, ok: true, data });
  } catch (error) {
    postMessage({
      type: 'result', id, ok: false,
      error: { message: error.message, reason: error.reason, params: error.params },
    });
  } finally {
    current = null;
  }
}

// Requests are queued rather than run as they arrive.
onmessage = (event) => {
  queue = queue.then(() => handle(event.data));
};
