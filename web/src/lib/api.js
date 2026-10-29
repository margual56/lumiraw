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
    if (type === 'tile') {
      call?.ontile?.(event.data.tile);
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
  const raw = Object.entries(body).filter(([, v]) => v instanceof ArrayBuffer || v instanceof Blob);
  const rest = Object.fromEntries(Object.entries(body).filter(([k]) => !raw.some(([r]) => r === k)));
  // `type` and `id` go on last: the callers still pass an `id` field of their
  // own (the session id the server used to hand out), and letting it win here
  // would address the reply to a call nobody is waiting for.
  const payload = { ...JSON.parse(JSON.stringify(rest)), ...Object.fromEntries(raw), type, id };
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject, ...hooks });
    ensure().postMessage(payload, transfer);
  });
}

/** Called with (fraction, stage code) for every stage of every operation. */
export function setProgressListener(fn) {
  listener = fn;
}

/** Warm the module up while the user is still looking at the drop zone. */
export function preload() {
  ready().catch(() => {});
}

/** Every photograph opened gets an id of its own. */
let opened = 0;
const photoId = () => `photo-${++opened}`;

export async function upload(file, onProgress) {
  onProgress?.(0);
  const buffer = await file.arrayBuffer();
  onProgress?.(1);
  const info = await call('open', { name: file.name, buffer }, {}, [buffer]);
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

/** The picture at full resolution, as an ImageBitmap, for the 100 % view. */
export const renderFull = (body) => call('full', body);
/**
 * A piece of the picture at full size, around `centre` (fractions of the framed
 * picture), at most `width` by `height`.
 */
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

/** Exports keep the job shape the download step already knows how to watch. */
const jobs = new Map();

export async function startExport(body) {
  const id = String(nextId++);
  const started = performance.now();
  const record = { state: 'running', progress: 0, stage: 'starting', stage_params: {}, elapsed: 0 };
  jobs.set(id, record);
  call('export', body, {
    onprogress: (fraction, code) => {
      record.progress = fraction;
      record.stage = code;
      record.elapsed = (performance.now() - started) / 1000;
    },
  })
    .then(({ blob, filename }) => Object.assign(record, {
      state: 'done', progress: 1, blob, filename,
      elapsed: (performance.now() - started) / 1000,
    }))
    .catch((error) => Object.assign(record, { state: 'error', error: error.message }));
  return { job: id };
}

export async function jobStatus(id) {
  const record = jobs.get(id);
  if (!record) throw new Error('lost track of the export');
  return { ...record };
}

export async function jobFile(id) {
  const record = jobs.get(id);
  if (!record?.blob) throw new Error('the finished file could not be fetched');
  return {
    blob: async () => record.blob,
    headers: {
      get: (name) => (name.toLowerCase() === 'content-disposition'
        ? `attachment; filename="${record.filename}"`
        : null),
    },
  };
}
