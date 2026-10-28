/** Scoring a gallery: each photograph made small, then judged by NIMA in a
 *  few workers at once. */

import * as api from './api.js';
import { RAW_ACCEPT } from './format.js';
import criticUrl from './wasm/critic.wasm?url';
import aestheticUrl from './wasm/critic/nima-aesthetic.bin?url';
import technicalUrl from './wasm/critic/nima-technical.bin?url';

/** The side both networks were trained at. */
const SIDE = 224;
/** The long edge of the picture kept for the grid. */
const SHOWN = 512;

const RAWS = RAW_ACCEPT.split(',');
export const IMAGES = ['.jpg', '.jpeg', '.png', '.webp'];
export const ACCEPT = [...RAWS, ...IMAGES].join(',');
export const isRaw = (name) => RAWS.some((s) => name.toLowerCase().endsWith(s));
export const isImage = (name) => IMAGES.some((s) => name.toLowerCase().endsWith(s));

let pool = null;
let nextId = 1;
const pending = new Map();

/** Fetch with progress, as (received, total) bytes. */
async function download(url, onbytes) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  const total = Number(response.headers.get('content-length')) || 0;
  if (!response.body) return response.arrayBuffer();
  const reader = response.body.getReader();
  const parts = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
    received += value.length;
    onbytes?.(received, total);
  }
  const out = new Uint8Array(received);
  let at = 0;
  for (const part of parts) { out.set(part, at); at += part.length; }
  return out.buffer;
}

function post(worker, type, body, transfer = []) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ type, id, ...body }, transfer);
  });
}

function receive({ data }) {
  const call = pending.get(data.id);
  if (!call) return;
  pending.delete(data.id);
  if (data.ok) call.resolve(data.data);
  else call.reject(new Error(data.error));
}

/**
 * Start the workers, once: the module and both networks are downloaded here
 * and copied to each. `onprogress` gets the fraction downloaded.
 */
export function ready(onprogress) {
  if (pool) return pool;
  pool = (async () => {
    const seen = new Map();
    const report = (url) => (received, total) => {
      seen.set(url, [received, total]);
      let got = 0;
      let all = 0;
      for (const [r, t] of seen.values()) { got += r; all += t || r; }
      if (all) onprogress?.(Math.min(1, got / all));
    };
    const [wasm, aesthetic, technical] = await Promise.all([
      download(criticUrl), download(aestheticUrl, report('a')), download(technicalUrl, report('t')),
    ]);
    const module = await WebAssembly.compile(wasm);
    // One core is left for the page and the decoder.
    const count = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1));
    const workers = [];
    for (let i = 0; i < count; i += 1) {
      const worker = new Worker(new URL('./wasm/critic-worker.js', import.meta.url),
                                { type: 'module' });
      worker.onmessage = receive;
      await post(worker, 'init', { module, aesthetic, technical });
      workers.push({ worker, busy: false });
    }
    return workers;
  })();
  pool.catch(() => { pool = null; });
  return pool;
}

/** A picture's pixels squashed to the networks' side, as idealo's code does. */
async function squash(source) {
  const bitmap = await createImageBitmap(source, {
    resizeWidth: SIDE, resizeHeight: SIDE, resizeQuality: 'high',
  });
  const canvas = new OffscreenCanvas(SIDE, SIDE);
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  return ctx.getImageData(0, 0, SIDE, SIDE).data.buffer;
}

/**
 * Everything the scoring needs from a file: a picture for the grid (an
 * object URL the caller revokes), the pixels to judge, and when it was taken.
 */
export async function prepare(file) {
  if (isRaw(file.name)) {
    const out = await api.preview(file, SHOWN);
    return { thumb: URL.createObjectURL(out.image), pixels: await squash(out.image),
             captured: out.captured_s || 0 };
  }
  // The browser decodes what it can show; orientation comes from the EXIF.
  const full = await createImageBitmap(file, { imageOrientation: 'from-image' });
  const scale = Math.min(1, SHOWN / Math.max(full.width, full.height));
  const canvas = new OffscreenCanvas(Math.round(full.width * scale), Math.round(full.height * scale));
  canvas.getContext('2d').drawImage(full, 0, 0, canvas.width, canvas.height);
  full.close();
  const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.85 });
  const [pixels, captured] = await Promise.all([squash(canvas), api.captured(file).catch(() => 0)]);
  return { thumb: URL.createObjectURL(blob), pixels, captured };
}

/** Calls waiting for a worker to come free. */
const waiting = [];

/** Judge one picture's pixels, on whichever worker is free. */
async function judge(pixels) {
  const workers = await pool;
  let free = workers.find((w) => !w.busy);
  if (free) free.busy = true;
  // A worker handed over by the call before is still marked busy.
  else free = await new Promise((resolve) => waiting.push(resolve));
  try {
    return await post(free.worker, 'score', { pixels, width: SIDE, height: SIDE }, [pixels]);
  } finally {
    const next = waiting.shift();
    if (next) next(free);
    else free.busy = false;
  }
}

/**
 * Score every file. Decoding goes one file at a time through the pipeline's
 * worker while the scoring workers judge what is already decoded.
 * `oneach(index, result | null, error)` as each finishes; `stopped()` is asked
 * between files.
 */
export async function scoreAll(files, { oneach, stopped = () => false }) {
  const workers = await pool;
  const inFlight = new Set();
  for (const [index, file] of files.entries()) {
    if (stopped()) break;
    let prepared;
    try {
      prepared = await prepare(file);
    } catch (error) {
      oneach(index, null, error);
      continue;
    }
    const job = judge(prepared.pixels)
      .then((score) => oneach(index, { ...score, thumb: prepared.thumb,
                                       captured: prepared.captured }))
      .catch((error) => { URL.revokeObjectURL(prepared.thumb); oneach(index, null, error); })
      .finally(() => inFlight.delete(job));
    inFlight.add(job);
    // Enough decoded ahead to keep every worker busy, and no more.
    while (inFlight.size >= workers.length * 2) await Promise.race(inFlight);
  }
  await Promise.all(inFlight);
}
