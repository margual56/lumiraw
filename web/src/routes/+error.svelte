<script>
  /** An address that is not a page. Served by the host with a 404 status
   *  (see svelte.config.js), and not to be indexed. */
  import { page } from '$app/state';
  import DocPage from '$lib/components/DocPage.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { localized, t } from '$lib/i18n.svelte.js';
</script>

<Meta title={t('missing.title')} description={t('missing.body')} index={false} />

<DocPage>
  <h1>{t('missing.title')}</h1>
  <p class="lede">{page.status === 404 ? t('missing.body') : page.error?.message}</p>
  <p>
    <a href={localized('/')}>{t('missing.home')}</a> ·
    <a href={localized('/guide')}>{t('links.guide')}</a> ·
    <a href={localized('/cameras')}>{t('links.cameras')}</a>
  </p>
</DocPage>
