<script>
  /** How assertively the automatic pipeline should act. */
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  const PRESETS = ['natural', 'punchy', 'flat'];
  const chosen = $derived(app.settings.preset ?? 'natural');
</script>

<div class="rendering">
  <h3>{t('render.title')}</h3>
  <div class="row" role="radiogroup" aria-label={t('render.title')}>
    {#each PRESETS as id}
      <button
        type="button"
        role="radio"
        aria-checked={chosen === id}
        onclick={() => (app.settings.preset = id)}
      >
        {t(`render.${id}`)}
      </button>
    {/each}
  </div>
  <p class="why">{t(`render.${chosen}.why`)}</p>
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .rendering { margin-top: 4px; }
  h3 {
    margin: 0 0 6px; @include section-label;
  }
  .row { display: flex; gap: 6px; }
  button {
    flex: 1; padding: 7px 4px; cursor: pointer;
    background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 8px;
    color: var(--color-muted); font: inherit; font-size: 12.5px; font-weight: 600;
    @include phone { font-size: 14px; }
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-checked="true"] {
      border-color: var(--color-accent); color: var(--color-ink);
      background: var(--color-panel);
    }
  }
  /* What the chosen one actually does, because three words on a button cannot
     say that this moves colour and sharpening as well as contrast. */
  .why {
    margin: 8px 0 0; color: var(--color-muted); @include small(11.8px); line-height: 1.5;
  }
</style>
