/** Other hosts' data, kept between visits so a reload does not ask again and a
 *  host that is down does not leave the page empty. On the site the service
 *  worker does this for every request. The desktop app has no service worker,
 *  so `remoteFetch` does it there, over a store on disk (desktop/src-tauri/src/cache.rs).
 *  OpenStreetMap's tiles are left to the browser: its policy is that clients
 *  follow its cache headers, and they are generous. */

const HOUR = 3600e3;
/** Which addresses, how long a copy needs no asking, and how long it may stand in when the host fails. */
const RULES = [
  ['https://api.open-meteo.com/v1/forecast', { fresh: HOUR, stale: 24 * HOUR }],
  ['https://s3.amazonaws.com/elevation-tiles-prod/terrarium/', { fresh: 720 * HOUR, stale: Infinity }],
  ['https://djlorenz.github.io/astronomy/', { fresh: 720 * HOUR, stale: Infinity }],
];
export const ruleFor = (url) => RULES.find(([prefix]) => url.startsWith(prefix))?.[1] ?? null;

/** `load(key)`, kept while the key stays among the last `keep` asked for. A
 *  failure is not kept, so the next ask tries again. */
export function lastAsked(keep, load) {
  const cache = new Map();
  return (key) => {
    const kept = cache.get(key);
    if (kept) {
      // Asked for again: to the back of the queue.
      cache.delete(key);
      cache.set(key, kept);
      return kept;
    }
    const loading = load(key);
    loading.catch(() => cache.delete(key));
    cache.set(key, loading);
    if (cache.size > keep) cache.delete(cache.keys().next().value);
    return loading;
  };
}

/** The desktop app's way to its own commands, or null on the site. */
const desktop = () => (typeof window === 'undefined' ? null : window.__TAURI_INTERNALS__?.invoke ?? null);

/** `fetch`, through the store on disk in the desktop app. */
export async function remoteFetch(url) {
  const invoke = desktop();
  const rule = ruleFor(url);
  if (!invoke || !rule) return fetch(url);

  // A store that cannot be read is only as good as having no copy.
  const kept = new Uint8Array(await invoke('cache_get', { url }).catch(() => new ArrayBuffer(0)));
  const age = kept.length ? Date.now() - Number(new DataView(kept.buffer).getBigUint64(0, true)) : Infinity;
  const copy = () => new Response(new Blob([kept.subarray(8)], { type: url.endsWith('.png') ? 'image/png' : '' }));
  if (age < rule.fresh) return copy();
  try {
    const response = await fetch(url);
    if (!response.ok) return age < rule.stale ? copy() : response;
    const bytes = new Uint8Array(await response.arrayBuffer());
    invoke('cache_put', bytes, { headers: { 'x-url': url } }).catch(() => {});
    return new Response(new Blob([bytes], { type: response.headers.get('content-type') ?? '' }));
  } catch (error) {
    if (age < rule.stale) return copy();
    throw error;
  }
}
