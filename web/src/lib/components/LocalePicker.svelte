<script>
  import { i18n, LOCALES, setLocale, t } from '$lib/i18n.svelte.js';
</script>

<label class="picker">
  <span class="sr">{t('locale.label')}</span>
  <select
    value={i18n.locale}
    aria-label={t('locale.label')}
    onchange={(event) => setLocale(event.currentTarget.value)}
  >
    {#each LOCALES as locale}
      <option value={locale.id}>{locale.name}</option>
    {/each}
  </select>
</label>

<style lang="scss">
  .picker { margin-left: auto; }
  select {
    width: auto; margin: 0; padding: 5px 8px; font-size: 12px;
    color: var(--color-muted); background: var(--color-panel-2);
    &:hover { color: var(--color-ink); }
  }
  /* Read out, never shown: the control is a bare select with a flag of a name. */
  .sr {
    position: absolute; width: 1px; height: 1px; overflow: hidden;
    clip-path: inset(50%); white-space: nowrap;
  }
</style>
