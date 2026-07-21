<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { profileRows, noteText } from '$lib/format.js';

  const rows = $derived(profileRows(app.info?.profile));
</script>

<details class="capture">
  <summary>{t('profile.title')}</summary>
  <dl>
    {#each rows as row}
      <dt>{row.label}</dt>
      <dd>{row.value}</dd>
    {/each}
    {#each app.info?.notes ?? [] as note}
      <div class="note">→ {noteText(note)}</div>
    {/each}
  </dl>
</details>

<style lang="scss">
  .capture { margin-top: 8px; border-top: 1px solid var(--color-line); padding-top: 12px; }
  summary { cursor: pointer; color: var(--color-muted); font-size: 12px; }
  dl {
    display: grid; grid-template-columns: auto 1fr; gap: 3px 12px;
    margin: 10px 0 0; font-size: 11.8px;
  }
  dt { color: var(--color-muted); }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  /* A remark about the capture itself, across both columns. */
  .note { grid-column: 1 / -1; color: var(--color-accent); margin-top: 8px; line-height: 1.45; }
</style>
