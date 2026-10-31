<script>
  import GuideLink from '$lib/components/GuideLink.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';
  import { setDeepDenoise } from '$lib/roll.svelte.js';

  /** Deep noise reduction needs the file itself, so a merge or a stack has none. */
  const canDeep = $derived(!!app.roll[app.rollAt]?.file);

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
  const noiseNow = $derived.by(() => {
    const colour = blend('denoise_chroma'), fine = blend('denoise_luma');
    return colour === t('grade.off') && fine === t('grade.off')
      ? t('finish.denoise.none') : t('finish.denoise.now', { colour, fine });
  });

  const offset = (v) => (v === 0 ? t('vibrance.auto') : signed(v));
  const share = (v) => `${Math.round(v * 100)}%`;

  /** Where the skin being protected came from, said plainly. */
  const skinNow = $derived.by(() => {
    const skin = app.settings.skin;
    if (skin === null) return t('skin.measuring');
    if (!skin?.faces) return t('skin.typical');
    return t('skin.measured', { count: skin.faces });
  });
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy} />
  <aside>
    <h2>{t('vibrance.title')}<GuideLink /></h2>
    <p class="lede">{t('vibrance.lede')}</p>
    <Slider label={t('vibrance.label')} bind:value={app.settings.vibrance}
            tone="vib" step={0.02} format={offset} />
    <Slider label={t('skin.protect')} bind:value={app.settings.skin_protect}
            min={0} max={1} step={0.05} format={share} />
    <p class="now">{skinNow}</p>

    <h3>{t('finish.title')}</h3>
    <p class="lede">{t('finish.lede')}</p>
    <Slider label={t('finish.clarity')} bind:value={app.settings.clarity}
            step={0.05} format={offset} />
    <Slider label={t('finish.dehaze')} bind:value={app.settings.dehaze}
            step={0.05} format={(v) => (v === 0 ? '0' : signed(v))} />
    <Slider label={t('finish.denoise')} bind:value={app.settings.denoise}
            step={0.05} format={offset} />
    <p class="now">{noiseNow}</p>
    {#if canDeep}
      <label class="check">
        <input type="checkbox" checked={app.settings.deep_denoise} disabled={app.busy}
               onchange={(e) => setDeepDenoise(e.currentTarget.checked)} />
        {t('finish.deep')}
      </label>
      <p class="now">{app.info?.deep_denoise === false ? t('finish.deep.unsupported') : t('finish.deep.why')}</p>
    {/if}
  </aside>
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  h3 {
    margin: 20px 0 6px; @include section-label;
  }
  .now { margin: -2px 0 0; @include small(12px); color: var(--color-muted); }
  .check {
    display: flex; gap: 8px; align-items: center; font-size: 13px; margin: 14px 0 4px;
    input { accent-color: var(--color-accent); }
  }
</style>
