/// <reference types="@sveltejs/kit" />
/** Offline, and installable: the whole application kept on this machine. */
import { build, files, prerendered, version } from '$service-worker';

const CACHE = `lumiraw-${version}`;
const IMMUTABLE = new Set(build);
// `_headers` and `_redirects` are instructions to the host, which never serves them.
const served = (f) => !/\/_[^/]*$/.test(f);
const ALL = [...build, ...files.filter(served), ...prerendered];
const onDemand = (path) => /\/(lens-[^/]*|lensfun-[^/]*)\.json$|\/look-[^/]*\.bin$/.test(path);

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
      if (key === CACHE) continue;
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
  if (url.origin !== self.location.origin) return;

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
