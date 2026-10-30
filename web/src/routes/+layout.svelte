<script>
  // The tokens and Tailwind first, then everything written in terms of them.
  import '../theme.css';
  import '../app.scss';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { dev } from '$app/environment';
  import { detectLocale, localized, unlocalized, useLocale } from '$lib/i18n.svelte.js';

  let { children } = $props();

  // Offline after the first visit, on the web. The desktop app (Tauri) serves
  // its own files from disk, so there is nothing for a worker to keep.
  $effect(() => {
    if (!('serviceWorker' in navigator) || '__TAURI_INTERNALS__' in window) return;
    // Bundled into one classic script for a build, as SvelteKit would register it.
    navigator.serviceWorker.register('/service-worker.js', { type: dev ? 'module' : 'classic' })
      .catch(() => {});
  });

  // The address says the language (/es/...
  useLocale(page.params.lang ?? 'en');
  $effect.pre(() => useLocale(page.params.lang ?? 'en'));

  // Someone arriving at an English address who reads Spanish, by their own
  // earlier choice or their browser's, is taken to the same page in Spanish.
  // Not inside a blog post (?embed, lib/demo.js): the post says the language.
  $effect(() => {
    if (page.params.lang || page.url.searchParams.has('embed')) return;
    if (detectLocale() === 'es' && page.status === 200) {
      goto(localized(unlocalized(page.url.pathname), 'es') + page.url.search + page.url.hash,
           { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{@render children()}
