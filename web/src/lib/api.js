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
  worker.postMessage({ type: 'init', id: 0 });
  return worker;
}

function call(type, body = {}, hooks = {}, transfer = []) {
  const id = nextId++;
  // Settings arrive as Svelte state proxies, which structured clone refuses.
  const { buffer, ...rest } = body;
  // `type` and `id` go on last: the callers still pass an `id` field of their
  // own (the session id the server used to hand out), and letting it win here
  // would address the reply to a call nobody is waiting for.
  const payload = { ...JSON.parse(JSON.stringify(rest)), type, id };
  if (buffer) payload.buffer = buffer;
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
  ensure();
}

export async function upload(file, onProgress) {
  onProgress?.(0);
  const buffer = await file.arrayBuffer();
  onProgress?.(1);
  const info = await call('open', { name: file.name, buffer }, {}, [buffer]);
  return { ...info, id: 'local', original_name: info.file || file.name };
}

export async function render(body) {
  const out = await call('render', body);
  return { ...out, image: toUrl(out.image) };
}

export async function compare(body) {
  const out = await call('compare', body);
  return { ...out, before: toUrl(out.before), after: toUrl(out.after) };
}

export async function styles(body, ontile) {
  const out = await call('styles', body, {
    ontile: ontile ? (tile) => ontile({ ...tile, image: toUrl(tile.image) }) : undefined,
  });
  return { tiles: out.tiles.map((tile) => ({ ...tile, image: toUrl(tile.image) })) };
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
