<script>
  /** The grade: one picture, one curve plot, and a short list of looks. */
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import CurvePlot from '$lib/components/CurvePlot.svelte';
  import CurveEditor from '$lib/components/CurveEditor.svelte';
  import CubeLoader from '$lib/components/CubeLoader.svelte';
  import ColourMixer from '$lib/components/ColourMixer.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { lookLabel, lookDescription } from '$lib/format.js';

  const curves = $derived(app.settings.curves);

  /** Load a look's points into the settings, which is what applies it. */
  function choose(look) {
    app.settings.curves = {
      look: look.id,
      strength: curves.strength ?? 1,
      rgb: look.points.rgb.map((p) => [...p]),
      r: look.points.r.map((p) => [...p]),
      g: look.points.g.map((p) => [...p]),
      b: look.points.b.map((p) => [...p]),
    };
  }

  const percent = (v) => `${Math.round(v * 100)}%`;
  const amount = (v) => (v > 0 ? percent(v) : t('grade.off'));
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy} />
  <aside>
    <h2>{t('grade.title')}</h2>
    <p class="lede">{@html t('grade.lede')}</p>

    <CurvePlot curve={app.curve} />

    <div class="list" role="radiogroup" aria-label={t('grade.title')}>
      {#each app.looks as look (look.id)}
        <button type="button" role="radio" aria-checked={curves.look === look.id}
                onclick={() => choose(look)}>
          <b>{lookLabel(look.id, look.label)}</b>
          <span>{lookDescription(look.id, look.description)}</span>
        </button>
      {/each}
    </div>

    <Slider label={t('grade.strength')} bind:value={app.settings.curves.strength}
            min={0} max={1} step={0.05} format={percent} />

    <CurveEditor />

    <ColourMixer />

    <CubeLoader />

    <h3>{t('grade.effects')}</h3>
    <p class="lede small">{t('grade.effects.why')}</p>
    <Slider label={t('grade.monochrome')} bind:value={app.settings.monochrome}
            min={0} max={1} step={0.05} format={amount} />
    <Slider label={t('grade.vignette')} bind:value={app.settings.vignette}
            min={0} max={1} step={0.05} format={amount} />
    <Slider label={t('grade.grain')} bind:value={app.settings.grain}
            min={0} max={1} step={0.05} format={amount} />

    <p class="points">
      {#if app.curve?.identity}
        {t('grade.nocurve')}
      {:else}
        {t('grade.points', { count: n(app.curve?.points
            ? Object.values(app.curve.points).reduce((a, b) => a + b, 0) : 0, 0) })}
      {/if}
    </p>
  </aside>
</div>

<style lang="scss">
  h3 {
    margin: 18px 0 4px; font-size: 10.5px; text-transform: uppercase;
    letter-spacing: .09em; color: var(--color-muted); font-weight: 600;
  }
  .small { font-size: 11.8px; margin-bottom: 10px; }
  .list { display: flex; flex-direction: column; gap: 6px; margin: 14px 0; }
  .list button {
    display: flex; flex-direction: column; gap: 1px; text-align: left;
    background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 8px;
    padding: 8px 10px; cursor: pointer; color: var(--color-ink); font: inherit;
    font-weight: 400;
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-checked="true"] { border-color: var(--color-accent); background: var(--color-panel); }
    b { font-size: 12.5px; font-weight: 600; }
    span { color: var(--color-muted); font-size: 11.5px; }
  }
  .points {
    margin: 6px 0 0; color: var(--color-muted); font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
</style>
