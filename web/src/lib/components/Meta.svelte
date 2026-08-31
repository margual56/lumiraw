<script>
  /** A page's title, description and link preview. */
  import { NAME, SITE } from '$lib/brand.js';

  let { title, description, path = '/', schema = null } = $props();

  const url = $derived(SITE + path);
  // `<` escaped so that nothing inside the data can close the script tag.
  const ld = $derived(schema
    ? `<script type="application/ld+json">${JSON.stringify(schema).replace(/</g, '\\u003c')}</` + 'script>'
    : '');
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href={url} />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content={NAME} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={url} />
  <meta property="og:image" content="{SITE}/og.png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta name="twitter:card" content="summary_large_image" />
  {@html ld}
</svelte:head>
