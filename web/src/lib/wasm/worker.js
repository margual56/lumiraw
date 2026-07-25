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

function withString(text, fn) {
  const bytes = encoder.encode(text);
  const ptr = copyIn(bytes);
  try {
    return fn(ptr, bytes.length);
  } finally {
    wasm.ar_free(ptr, bytes.length);
  }
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

async function render({ settings, size, uncropped, style }) {
  const rc = settingsPointer(settings, (sp, sl) =>
    withString(style || 'original', (yp, yl) =>
      wasm.ar_render(sp, sl, size ?? 1300, uncropped ? 0 : 1, yp, yl)));
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

async function styles({ settings, size }) {
  const count = settingsPointer(settings, (sp, sl) => wasm.ar_styles_prepare(sp, sl, size ?? 1200));
  const prepared = readJson();
  if (count < 0) fail(prepared);
  const tiles = [];
  for (let i = 0; i < count; i += 1) {
    if (wasm.ar_style_tile(i) !== 0) fail(readJson());
    const meta = readJson();
    const { w, h, pixels } = primary();
    tiles.push({
      id: meta.id,
      label: meta.label,
      description: meta.description,
      image: await toBlob(pixels, w, h),
    });
    // Let the page paint each tile as it lands rather than after all ten.
    postMessage({ type: 'tile', id: current, tile: tiles[tiles.length - 1] });
  }
  return { tiles };
}

async function exportImage({ settings, styles: chosen, format, quality, max_size, original_name }) {
  // One time for the whole export, so several looks saved together agree.
  const now = stamp();
  const wanted = chosen?.length ? chosen : ['original'];
  const stem = (original_name || 'photo').replace(/\.[^.]+$/, '');
  const files = [];
  for (const style of wanted) {
    const rc = settingsPointer(settings, (sp, sl) =>
      withString(style, (yp, yl) =>
        withString(format, (fp, fl) =>
          withString(now, (np, nl) =>
            wasm.ar_export(sp, sl, yp, yl, fp, fl, quality ?? 92,
                           max_size ? Number(max_size) : 0, np, nl)))));
    const info = readJson();
    if (rc !== 0) fail(info);
    let blob;
    if (info.canvas) {
      // WebP has no pure-Rust encoder worth carrying, so the browser's own
      // encoder finishes the job.
      const { w, h, pixels } = primary();
      blob = await toBlob(pixels, w, h, 'image/webp', (quality ?? 92) / 100);
      if (info.exif) {
        blob = await withExif(blob, encodedBytes(), w, h);
      }
    } else {
      blob = new Blob([encodedBytes()], { type: info.mime });
    }
    const suffix = style === 'original' ? '' : `_${style}`;
    files.push({ name: `${stem}${suffix}.${info.ext ?? 'webp'}`, blob,
                 captured: info.captured });
  }
  if (files.length === 1) {
    return { blob: files[0].blob, filename: files[0].name };
  }
  return { blob: await zip(files, now), filename: `${stem}_styles.zip` };
}

/**
 * EXIF spells a time `YYYY:MM:DD HH:MM:SS`, with no zone, meaning local time
 * where the photograph was taken.
 */
function fromExif(text) {
  const m = /^(\d{4}):(\d{2}):(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/.exec(text ?? '');
  if (!m) return null;
  const [, y, mo, d, h, mi, sec] = m.map(Number);
  const when = new Date(y, mo - 1, d, h, mi, sec);
  return Number.isNaN(when.getTime()) ? null : when;
}

/** A date as MS-DOS packs it, which is what a zip entry carries. */
function dosTime(when) {
  const year = Math.max(1980, when.getFullYear());
  return {
    date: ((year - 1980) << 9) | ((when.getMonth() + 1) << 5) | when.getDate(),
    time: (when.getHours() << 11) | (when.getMinutes() << 5) | (when.getSeconds() >> 1),
  };
}

/** The extended timestamp and NTFS extra fields, which carry the times the DOS field cannot. */
function timeExtras(modified, created, { local }) {
  const unix = (when) => Math.floor(when.getTime() / 1000);
  // 0x5455, with bit 0 for modification and bit 2 for creation.
  const flags = created ? 0b101 : 0b001;
  const times = local && created ? [unix(modified), unix(created)] : [unix(modified)];
  const ut = new DataView(new ArrayBuffer(5 + times.length * 4));
  ut.setUint16(0, 0x5455, true);
  ut.setUint16(2, 1 + times.length * 4, true);
  ut.setUint8(4, flags);
  times.forEach((t, i) => ut.setInt32(5 + i * 4, t, true));

  // 0x000a, as Windows file times: 100 ns ticks since 1601.
  const filetime = (when) => (BigInt(when.getTime()) + 11644473600000n) * 10000n;
  const ntfs = new DataView(new ArrayBuffer(36));
  ntfs.setUint16(0, 0x000a, true);
  ntfs.setUint16(2, 32, true);
  ntfs.setUint16(8, 0x0001, true);
  ntfs.setUint16(10, 24, true);
  ntfs.setBigUint64(12, filetime(modified), true);
  ntfs.setBigUint64(20, filetime(modified), true);
  ntfs.setBigUint64(28, filetime(created ?? modified), true);

  const out = new Uint8Array(ut.buffer.byteLength + 36);
  out.set(new Uint8Array(ut.buffer), 0);
  out.set(new Uint8Array(ntfs.buffer), ut.buffer.byteLength);
  return out;
}

/**
 * Stored-only ZIP: the members are already compressed images, so deflating them
 * again would cost seconds and save nothing.
 */
async function zip(files, now) {
  const parts = [];
  const central = [];
  let offset = 0;
  const table = crcTable();
  const modified = fromExif(now) ?? new Date();
  const stamp = dosTime(modified);
  for (const file of files) {
    const name = encoder.encode(file.name);
    const created = fromExif(file.captured);
    const extraLocal = timeExtras(modified, created, { local: true });
    const extraDir = timeExtras(modified, created, { local: false });
    const data = new Uint8Array(await file.blob.arrayBuffer());
    const crc = crc32(data, table);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint16(6, 0, true);
    local.setUint16(8, 0, true);          // stored
    local.setUint16(10, stamp.time, true);
    local.setUint16(12, stamp.date, true);
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, name.length, true);
    local.setUint16(28, extraLocal.length, true);
    parts.push(new Uint8Array(local.buffer), name, extraLocal, data);

    const dir = new DataView(new ArrayBuffer(46));
    dir.setUint32(0, 0x02014b50, true);
    dir.setUint16(4, 20, true);
    dir.setUint16(6, 20, true);
    dir.setUint16(10, 0, true);
    dir.setUint16(12, stamp.time, true);
    dir.setUint16(14, stamp.date, true);
    dir.setUint32(16, crc, true);
    dir.setUint32(20, data.length, true);
    dir.setUint32(24, data.length, true);
    dir.setUint16(28, name.length, true);
    dir.setUint16(30, extraDir.length, true);
    dir.setUint32(42, offset, true);
    central.push(new Uint8Array(dir.buffer), name, extraDir);
    offset += 30 + name.length + extraLocal.length + data.length;
  }
  const centralSize = central.reduce((n, part) => n + part.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, files.length, true);
  end.setUint16(10, files.length, true);
  end.setUint32(12, centralSize, true);
  end.setUint32(16, offset, true);
  return new Blob([...parts, ...central, new Uint8Array(end.buffer)], { type: 'application/zip' });
}

function crcTable() {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let c = i;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[i] = c >>> 0;
  }
  return table;
}

function crc32(bytes, table) {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i += 1) c = table[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** Add one exposure to the bracket waiting to be merged. */
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
  const rc = wasm.ar_merge_finish(align ? 1 : 0, deghost ?? 0.5);
  const info = readJson();
  if (rc !== 0) fail(info);
  return info;
}

const HANDLERS = { open, render, compare, styles, export: exportImage,
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
