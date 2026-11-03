/// <reference types="@sveltejs/kit" />
/** Offline, and installable: the whole application kept on this machine. */
import { build, files, prerendered, version } from '$service-worker';
import { ruleFor } from './lib/remote.js';

const CACHE = `lumiraw-${version}`;
const IMMUTABLE = new Set(build);
// `_headers` and `_redirects` are instructions to the host, which never serves them.
const served = (f) => !/\/_[^/]*$/.test(f);
const ALL = [...build, ...files.filter(served), ...prerendered];
// The scoring and face networks too: 16 MB that only the gallery page ever wants.
const onDemand = (path) =>
  /\/(lens-[^/]*|lensfun-[^/]*)\.json$|\/((nima|face)-[^/]*|segmenter|denoiser)\.bin$/.test(path);

// Other hosts' data (see lib/remote.js)
const REMOTE = 'lumiraw-remote-v1';
const SAVED = 'x-lumiraw-saved';
/** Enough for many spots; the oldest go first. */
const REMOTE_ENTRIES = 500;

async function remote(url, { fresh, stale }) {
  const cache = await caches.open(REMOTE);
  const hit = await cache.match(url);
  const age = hit ? Date.now() - Number(hit.headers.get(SAVED)) : Infinity;
  if (age < fresh) return hit;
  try {
    // Readable even for the map's <img> tiles, so that its age can be kept with it.
    const response = await fetch(url, { mode: 'cors', credentials: 'omit' });
    if (!response.ok) return age < stale ? hit : response;
    const headers = new Headers(response.headers);
    headers.set(SAVED, String(Date.now()));
    const copy = new Response(await response.blob(), { status: response.status, statusText: response.statusText, headers });
    await cache.delete(url);
    await cache.put(url, copy.clone());
    const keys = await cache.keys();
    await Promise.all(keys.slice(0, Math.max(0, keys.length - REMOTE_ENTRIES)).map((key) => cache.delete(key)));
    return copy;
  } catch (err) {
    if (age < stale) return hit;
    throw err;
  }
}

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    // The code and the engine must all be there, or offline is a promise the
    // page cannot keep.
    await cache.addAll([...build.filter((f) => !onDemand(f)),
                        ...files.filter((f) => served(f) && !f.includes('/samples/'))]);
    // Pages are best effort, each on its own.
    await Promise.all(prerendered.flatMap((page) => [page, `${page.replace(/\/$/, '')}.html`])
      .map((url) => cache.add(url).catch(() => {})));
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    for (const key of await caches.keys()) {
      if (key === CACHE || key === REMOTE) continue;
      // Lens pieces fetched under the last deploy, still wanted by this one.
      const old = await caches.open(key);
      for (const path of build.filter(onDemand)) {
        const hit = await old.match(path);
        if (hit) await cache.put(path, hit);
      }
      await caches.delete(key);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) {
    const rule = ruleFor(request.url);
    if (rule) event.respondWith(remote(request.url, rule));
    return;
  }

  if (IMMUTABLE.has(url.pathname)) {
    event.respondWith((async () => {
      const hit = await caches.match(request);
      if (hit) return hit;
      const response = await fetch(request);
      if (response.ok && onDemand(url.pathname)) {
        const cache = await caches.open(CACHE);
        await cache.put(request, response.clone());
      }
      return response;
    })());
    return;
  }
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    // The page as it was cached, found by its own path or as the prerendered
    // file behind it (`/merge` is served from `/merge.html`).
    const cached = async () => await cache.match(request, { ignoreSearch: true })
      ?? await cache.match(`${url.pathname.replace(/\/$/, '')}.html`)
      ?? (url.pathname.endsWith('/') ? await cache.match(`${url.pathname}index.html`) : null);
    try {
      const response = await fetch(request);
      if (response.ok) {
        if (ALL.includes(url.pathname)) cache.put(request, response.clone());
        return response;
      }
      // A host that does not map `/merge` to its file answers 404 for a
      // page this worker has; the page wins.
      return (await cached()) ?? response;
    } catch (err) {
      const hit = await cached();
      if (hit) return hit;
      throw err;
    }
  })());
});
