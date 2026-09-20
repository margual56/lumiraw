<script>
  /** An address that is not a page. */
  import { page } from '$app/state';
  import DocPage from '$lib/components/DocPage.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { t } from '$lib/i18n.svelte.js';
</script>

<Meta title={t('missing.title')} description={t('missing.body')} index={false} />

<DocPage>
  <h1>{t('missing.title')}</h1>
  <p class="lede">{page.status === 404 ? t('missing.body') : page.error?.message}</p>
</DocPage>
