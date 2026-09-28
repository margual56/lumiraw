<script>
  /** The grade: one picture, a short list of looks, and the tools to go on from one. */
  import Viewer from '$lib/components/Viewer.svelte';
  import Compare from '$lib/components/Compare.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import CurvePlot, { moves } from '$lib/components/CurvePlot.svelte';
  import CurveEditor from '$lib/components/CurveEditor.svelte';
  import CubeLoader from '$lib/components/CubeLoader.svelte';
  import ColourMixer from '$lib/components/ColourMixer.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { lookLabel, lookDescription } from '$lib/format.js';

  /** The picture with and without the grade, or the picture alone, which
   *  is where the 100 % view and the histogram are. */
  let wipe = $state(true);

  /** A look is named, not copied in: its table stays in the worker, and the
   *  curves below stay the photographer's own, drawn on top of it. */
  function choose(look) {
    app.settings.look = look.id === 'none' ? '' : look.id;
  }

  const percent = (v) => `${Math.round(v * 100)}%`;
  const amount = (v) => (v > 0 ? percent(v) : t('grade.off'));
</script>

<div class="stage">
  <div class="view">
    {#if wipe}
      <Compare before={app.ungraded.image} after={app.preview.image} busy={app.busy}
               beforeLabel={t('grade.without')} afterLabel={t('grade.with')} />
    {:else}
      <Viewer src={app.preview.image} busy={app.busy} />
    {/if}
    <div class="mode" role="radiogroup" aria-label={t('grade.view')}>
      <button type="button" role="radio" aria-checked={wipe} onclick={() => (wipe = true)}>
        {t('grade.view.wipe')}</button>
      <button type="button" role="radio" aria-checked={!wipe} onclick={() => (wipe = false)}>
        {t('grade.view.single')}</button>
    </div>
  </div>
  <aside>
    <h2>{t('grade.title')}</h2>
    <p class="lede">{@html t('grade.lede')}</p>

    <!-- An empty grid with only the diagonal in it reads as something that
         failed to load; the plot appears once there is a curve to show. -->
    {#if app.curve && ['rgb', 'r', 'g', 'b'].some((k) => moves(app.curve[k]))}
      <CurvePlot curve={app.curve} />
    {/if}

    <div class="list" role="radiogroup" aria-label={t('grade.title')}>
      {#each app.looks as look (look.id)}
        <button type="button" role="radio" aria-checked={(app.settings.look || 'none') === look.id}
                onclick={() => choose(look)}>
          <b>{lookLabel(look.id, look.label)}</b>
          <span>{lookDescription(look.id, look.description)}</span>
        </button>
      {/each}
    </div>

    {#if app.settings.look}
      <Slider label={t('grade.strength')} bind:value={app.settings.look_strength}
              min={0} max={1} step={0.05} format={percent} />
    {/if}

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
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .view { position: relative; min-width: 0; }
  .mode {
    display: flex; gap: 4px; margin-top: 8px; justify-content: center;
    button {
      padding: 4px 12px; font-size: 12px; font-weight: 500; min-width: 0;
      @include phone { padding: 8px 14px; min-height: 40px; font-size: 13px; }
      background: var(--color-panel-2); color: var(--color-muted); border: 1px solid var(--color-line);
      &[aria-checked='true'] { color: var(--color-ink); border-color: var(--color-accent); }
    }
  }
  h3 {
    margin: 18px 0 4px; @include section-label;
  }
  .small { @include small(11.8px); margin-bottom: 10px; }
  .list { display: flex; flex-direction: column; gap: 6px; margin: 14px 0; }
  .list button {
    display: flex; flex-direction: column; gap: 1px; text-align: left;
    background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 8px;
    padding: 8px 10px; cursor: pointer; color: var(--color-ink); font: inherit;
    font-weight: 400;
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-checked="true"] { border-color: var(--color-accent); background: var(--color-panel); }
    b { font-size: 12.5px; font-weight: 600; @include phone { font-size: 14px; } }
    span { color: var(--color-muted); @include small(11.5px); }
    @include phone { padding: 10px 12px; }
  }
  .points {
    margin: 6px 0 0; color: var(--color-muted); @include small(11.5px);
    font-variant-numeric: tabular-nums;
  }
</style>
