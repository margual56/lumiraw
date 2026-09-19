<script>
  /** A page's title, description and link preview, and its other language. */
  import { NAME, SITE } from '$lib/brand.js';
  import { address, i18n } from '$lib/i18n.svelte.js';

  let { title, description, path = '/', schema = null, index = true } = $props();

  const url = $derived(SITE + address(path));
  const english = $derived(SITE + address(path, 'en'));
  const spanish = $derived(SITE + address(path, 'es'));
  const ogLocale = $derived(i18n.locale === 'es' ? 'es_ES' : 'en_GB');
  // `<` escaped so that nothing inside the data can close the script tag.
  const ld = $derived(schema
    ? `<script type="application/ld+json">${JSON.stringify(schema).replace(/</g, '\\u003c')}</` + 'script>'
    : '');
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  {#if index}
    <link rel="canonical" href={url} />
    <link rel="alternate" hreflang="en" href={english} />
    <link rel="alternate" hreflang="es" href={spanish} />
    <link rel="alternate" hreflang="x-default" href={english} />
  {:else}
    <meta name="robots" content="noindex" />
  {/if}
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content={NAME} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={url} />
  <meta property="og:locale" content={ogLocale} />
  <meta property="og:locale:alternate" content={ogLocale === 'es_ES' ? 'en_GB' : 'es_ES'} />
  <meta property="og:image" content="{SITE}/og.png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta name="twitter:card" content="summary_large_image" />
  {@html ld}
</svelte:head>
