<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';

  /**
   * What noise reduction is doing to this frame right now, as the pipeline
   * reported it, so the slider below reads as "more or less than this" rather
   * than as a number with nothing to compare it to.
   */
  const blend = (id) => {
    const toggle = app.toggles.find((x) => x.id === id);
    const b = toggle?.enabled ? toggle.detail?.params?.blend : null;
    return typeof b === 'number' ? `${Math.round(b * 100)}%` : t('grade.off');
  };
  const noiseNow = $derived(t('finish.denoise.now', {
    colour: blend('denoise_chroma'), fine: blend('denoise_luma') }));

  const offset = (v) => (v === 0 ? t('vibrance.auto') : signed(v));
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy} />
  <aside>
    <h2>{t('vibrance.title')}</h2>
    <p class="lede">{t('vibrance.lede')}</p>
    <Slider label={t('vibrance.label')} bind:value={app.settings.vibrance}
            tone="vib" step={0.02} format={offset} />

    <h3>{t('finish.title')}</h3>
    <p class="lede">{t('finish.lede')}</p>
    <Slider label={t('finish.clarity')} bind:value={app.settings.clarity}
            step={0.05} format={offset} />
    <Slider label={t('finish.denoise')} bind:value={app.settings.denoise}
            step={0.05} format={offset} />
    <p class="now">{noiseNow}</p>
  </aside>
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  h3 {
    margin: 20px 0 6px; @include section-label;
  }
  .now { margin: -2px 0 0; @include small(12px); color: var(--color-muted); }
</style>
