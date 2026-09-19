<script>
  // The tokens and Tailwind first, then everything written in terms of them.
  import '../theme.css';
  import '../app.scss';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { detectLocale, localized, unlocalized, useLocale } from '$lib/i18n.svelte.js';

  let { children } = $props();

  // The address says the language (/es/...
  useLocale(page.params.lang ?? 'en');
  $effect.pre(() => useLocale(page.params.lang ?? 'en'));

  // Someone arriving at an English address who reads Spanish, by their own
  // earlier choice or their browser's, is taken to the same page in Spanish.
  $effect(() => {
    if (page.params.lang) return;
    if (detectLocale() === 'es' && page.status === 200) {
      goto(localized(unlocalized(page.url.pathname), 'es') + page.url.search + page.url.hash,
           { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{@render children()}
