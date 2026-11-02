<script>
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { i18n, LOCALES, localized, setLocale, t, unlocalized } from '$lib/i18n.svelte.js';

  /** Remember the choice, and go to the same page in that language. */
  function choose(id) {
    setLocale(id);
    // From the address itself: the calculators keep their tab and spot in its
    // hash without telling the router.
    goto(localized(unlocalized(page.url.pathname), id) + location.search + location.hash,
         { noScroll: true, keepFocus: true });
  }
</script>

<label class="picker">
  <span class="sr">{t('locale.label')}</span>
  <select
    value={i18n.locale}
    aria-label={t('locale.label')}
    onchange={(event) => choose(event.currentTarget.value)}
  >
    {#each LOCALES as locale}
      <option value={locale.id}>{locale.name}</option>
    {/each}
  </select>
</label>

<style lang="scss">
  select {
    width: auto; margin: 0; padding: 5px 26px 5px 8px; font-size: 12px;
    color: var(--color-muted); background-color: var(--color-panel-2);
    background-position: right 9px center;
    &:hover { color: var(--color-ink); }
  }
  /* Read out, never shown: the control is a bare select with a flag of a name. */
  .sr {
    position: absolute; width: 1px; height: 1px; overflow: hidden;
    clip-path: inset(50%); white-space: nowrap;
  }
</style>
