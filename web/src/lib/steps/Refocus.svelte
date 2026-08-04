<script>
  /** The step that only exists for a frame that missed focus. */
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';

  const focus = $derived(app.focus ?? {});
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy} />
  <aside>
    <h2>{t('refocus.title')}</h2>
    <p class="lede">{@html t('refocus.lede')}</p>

    <p class="measured">
      {t('refocus.measured', { edge: n(focus.edge_px ?? 0, 1),
                               blur: n(focus.blur_px ?? 0, 1) })}
    </p>

    <Slider label={t('refocus.amount')} bind:value={app.settings.refocus}
            min={0} max={1} step={0.05} format={(v) => (v > 0 ? n(v, 2) : t('refocus.off'))} />
    <p class="lede hint">{t('refocus.hint')}</p>

    <button class="ghost" onclick={() => (app.settings.refocus = 0)}
            disabled={!app.settings.refocus}>
      {t('refocus.reset')}
    </button>
  </aside>
</div>

<style lang="scss">
  .hint { margin: -8px 0 14px; }
  .measured {
    margin: 0 0 14px; padding: 10px 12px; border-radius: 10px;
    background: var(--color-panel-2); color: var(--color-muted);
    font-size: 13px; line-height: 1.5;
  }
</style>
