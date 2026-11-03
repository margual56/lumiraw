/** The pipeline, called in the browser. */

let worker = null;
let nextId = 1;
const pending = new Map();
let listener = null;

/** Preview images are object URLs.  Nothing owns them for long, so a small
 *  ring keeps the last handful alive and revokes the rest. */
const urls = [];
function toUrl(blob) {
  const url = URL.createObjectURL(blob);
  urls.push(url);
  while (urls.length > 40) URL.revokeObjectURL(urls.shift());
  return url;
}

function ensure() {
  if (worker) return worker;
  worker = new Worker(new URL('./wasm/worker.js', import.meta.url), { type: 'module' });
  worker.onmessage = (event) => {
    const { type, id } = event.data;
    const call = pending.get(id);
    if (type === 'progress') {
      call?.onprogress?.(event.data.fraction, event.data.code);
      listener?.(event.data.fraction, event.data.code);
      return;
    }
    if (type !== 'result' || !call) return;
    pending.delete(id);
    if (event.data.ok) {
      call.resolve(event.data.data);
    } else {
      const error = new Error(event.data.error.message);
      error.reason = event.data.error.reason;
      error.params = event.data.error.params;
      call.reject(error);
    }
  };
  worker.onerror = (event) => {
    for (const [id, call] of pending) {
      pending.delete(id);
      call.reject(new Error(event.message || 'the pipeline worker stopped'));
    }
  };
  return worker;
}

/** Resolves once the module is loaded, with what the build reports about itself. */
let readyPromise = null;
export function ready() {
  if (!readyPromise) readyPromise = call('init');
  return readyPromise;
}

function call(type, body = {}, hooks = {}, transfer = []) {
  const id = nextId++;
  // Settings arrive as Svelte state proxies, which structured clone refuses,
  // so they go through JSON. Bytes and files go over as they are: cloning a
  // File passes a reference, not its contents.
  const bytes = (v) => v instanceof ArrayBuffer || v instanceof Blob;
  const raw = Object.entries(body).filter(([, v]) => bytes(v));
  const rest = Object.entries(body).filter(([, v]) => !bytes(v));
  const payload = { ...JSON.parse(JSON.stringify(Object.fromEntries(rest))),
                    ...Object.fromEntries(raw), type, id };
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject, ...hooks });
    ensure().postMessage(payload, transfer);
  });
}

/** Called with (fraction, stage code) for every stage of every operation. */
export function setProgressListener(fn) {
  listener = fn;
}

let opened = 0;
const photoId = () => `photo-${++opened}`;

/** Open a raw for developing; `deep` runs the deep noise reduction on its
 *  mosaic first, which takes a while. */
export async function upload(file, { deep = false } = {}) {
  const buffer = await file.arrayBuffer();
  const info = await call('open', { name: file.name, buffer, deep }, {}, [buffer]);
  return { ...info, id: photoId(), original_name: info.file || file.name };
}

export async function render(body) {
  const out = await call('render', body);
  return { ...out, image: toUrl(out.image) };
}

/** A small picture of a file for the filmstrip, from its embedded preview,
 *  as an object URL the caller owns (and revokes). */
export async function thumbnail(file, size = 240) {
  const buffer = await file.arrayBuffer();
  const out = await call('thumbnail', { buffer, size }, {}, [buffer]);
  return URL.createObjectURL(out.image);
}

/** A raw's embedded preview at about `size` pixels, uncompressed, and when
 *  it was taken: { pixels (RGBA), width, height, captured_s }. */
export async function preview(file, size) {
  const buffer = await file.arrayBuffer();
  return call('preview', { buffer, size }, {}, [buffer]);
}

/** When a JPEG, PNG or WebP says it was taken, in seconds, or zero. The EXIF
 *  is near the start, so only that much goes to the module. */
export async function captured(file) {
  const buffer = await file.slice(0, 256 * 1024).arrayBuffer();
  const out = await call('captured', { buffer }, {}, [buffer]);
  return out.captured_s ?? 0;
}

/** One export, awaited: { blob, filename, captured }. For the batch, which
 *  reports its own progress per photograph. */
export const exportOnce = (body, onprogress) => call('export', body, { onprogress });

/** A piece of the picture at full size, around `centre` (fractions of the
 *  framed picture), at most `width` by `height`, for the 100 % view. */
export const renderRegion = (body) => call('region', body);
export const releaseRegion = () => call('releaseRegion');

export async function compare(body) {
  const out = await call('compare', body);
  return { ...out, before: toUrl(out.before), after: toUrl(out.after) };
}

/** The named grades and their control points, once per session. */
export const looks = () => call('looks');

/** The side a reference picture is read at for matching its colour. */
const REFERENCE = 512;

/**
 * Wheels that bring the open photograph's colour toward a reference picture:
 * a raw (its embedded preview) or a JPEG, PNG or WebP.
 */
export async function matchColour(file, settings) {
  let pixels, width, height;
  if (/\.(jpe?g|png|webp|avif|gif)$/i.test(file.name) || file.type.startsWith('image/')) {
    const bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' });
    const scale = Math.min(1, REFERENCE / Math.max(bitmap.width, bitmap.height));
    width = Math.max(1, Math.round(bitmap.width * scale));
    height = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext('2d');
    ctx.drawImage(bitmap, 0, 0, width, height);
    bitmap.close();
    pixels = ctx.getImageData(0, 0, width, height).data.buffer;
  } else {
    ({ pixels, width, height } = await preview(file, REFERENCE));
  }
  return call('match', { settings, pixels, width, height }, {}, [pixels]);
}

/** Where a filter following the sky or the subject reaches, as grey RGBA
 *  over the framed picture: `{ pixels, width, height }`. */
export const mask = (settings, kind) => call('mask', { settings, kind });

/** The skin of the open photograph's faces: `{hue, chroma, faces}`, or
 *  `{faces: 0}` when there are none. */
export const skin = (settings) => call('skin', { settings });

/** The four curves the pipeline will evaluate, baked, for drawing. */
export const curves = (body) => call('curves', body);

/** Read a `.cube` file into the module. The bytes go no further than the tab. */
export async function lutLoad(file) {
  const buffer = await file.arrayBuffer();
  return call('lutLoad', { buffer }, {}, [buffer]);
}

export const lutClear = () => call('lutClear');

/** Building a merge: the frames go over one at a time, then one call to
 *  combine them. The result replaces whatever the wizard was developing. */
export const mergeReset = () => call('mergeReset');
export const mergeRemove = (index) => call('mergeRemove', { index });
export const mergeCheck = () => call('mergeCheck');

export async function mergeAdd(file) {
  const buffer = await file.arrayBuffer();
  return call('mergeAdd', { name: file.name, buffer }, {}, [buffer]);
}

export async function mergeFinish(options) {
  const info = await call('mergeFinish', options);
  return { ...info, id: photoId(), original_name: info.file };
}

/** How a file says it was taken (shutter, aperture, ISO, when), from its
 *  first bytes, without decoding it. */
export async function exposure(file) {
  const buffer = await file.slice(0, 512 * 1024).arrayBuffer();
  return call('exposure', { buffer }, {}, [buffer]);
}

/** Building a stack: begun with its mode, then the files one at a time (the
 *  worker reads each and lets it go), then one call to finish. The result
 *  replaces whatever the wizard was developing. */
export const stackBegin = (mode, align) => call('stackBegin', { mode, align });
export const stackAdd = (file) => call('stackAdd', { file });
export const stackReset = () => call('stackReset');

export async function stackFinish() {
  const info = await call('stackFinish');
  return { ...info, id: photoId(), original_name: info.file };
}
