<script>
  /** The grade: one picture, a short list of looks, and the tools to go on from one. */
  import GuideLink from '$lib/components/GuideLink.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import Compare from '$lib/components/Compare.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import CurveEditor from '$lib/components/CurveEditor.svelte';
  import CubeLoader from '$lib/components/CubeLoader.svelte';
  import ColourMixer from '$lib/components/ColourMixer.svelte';
  import ColourGrading from '$lib/components/ColourGrading.svelte';
  import Section from '$lib/components/Section.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { lookLabel, lookDescription } from '$lib/format.js';
  import { lookControls } from '$lib/looks.js';

  /** The picture with and without the grade, or the picture alone, which
   *  is where the 100 % view and the histogram are. */
  let wipe = $state(true);

  /** A look is a setting of the controls below: choosing one fills in the
   *  curves, the mixer, the wheels and monochrome, and from there they are
   *  the photographer's to move. "None" puts them back to nothing. */
  function choose(look) {
    const id = look.id === 'none' ? '' : look.id;
    app.settings = { ...app.settings, ...lookControls(id), look: id };
  }

  const chosen = $derived(app.looks.find((l) => l.id === (app.settings.look || 'none')));
  const effected = $derived(app.settings.monochrome > 0 || app.settings.vignette > 0
                            || app.settings.grain > 0);

  const percent = (v) => `${Math.round(v * 100)}%`;
  const amount = (v) => (v > 0 ? percent(v) : t('grade.off'));
</script>

<div class="stage">
  <div class="view">
    {#if wipe}
      <Compare before={app.ungraded.image} after={app.preview.image} busy={app.busy && !app.live}
               beforeLabel={t('grade.without')} afterLabel={t('grade.with')} />
    {:else}
      <Viewer src={app.preview.image} busy={app.busy && !app.live} />
    {/if}
    <div class="mode" role="radiogroup" aria-label={t('grade.view')}>
      <button type="button" role="radio" aria-checked={wipe} onclick={() => (wipe = true)}>
        {t('grade.view.wipe')}</button>
      <button type="button" role="radio" aria-checked={!wipe} onclick={() => (wipe = false)}>
        {t('grade.view.single')}</button>
    </div>
  </div>
  <aside>
    <h2>{t('grade.title')}<GuideLink /></h2>
    <p class="lede">{@html t('grade.lede')}</p>

    <div class="looks" role="radiogroup" aria-label={t('grade.looks')}>
      {#each app.looks as look (look.id)}
        <button type="button" role="radio" aria-checked={(app.settings.look || 'none') === look.id}
                title={lookDescription(look.id, look.description)} onclick={() => choose(look)}>
          {lookLabel(look.id, look.label)}
        </button>
      {/each}
    </div>
    {#if chosen}
      <p class="about">{lookDescription(chosen.id, chosen.description)}</p>
    {/if}

    <div class="tools">
      <CurveEditor />
      <ColourMixer />
      <ColourGrading />
      <Section title={t('grade.effects')} edited={effected}>
        <Slider label={t('grade.monochrome')} bind:value={app.settings.monochrome}
                min={0} max={1} step={0.05} format={amount} />
        <Slider label={t('grade.vignette')} bind:value={app.settings.vignette}
                min={0} max={1} step={0.05} format={amount} />
        <Slider label={t('grade.grain')} bind:value={app.settings.grain}
                min={0} max={1} step={0.05} format={amount} />
      </Section>
      <CubeLoader />
    </div>
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
  .looks {
    display: grid; grid-template-columns: 1fr 1fr; gap: 4px; margin: 12px 0 0;
    button {
      padding: 7px 8px; min-width: 0; text-align: left; font: inherit; font-size: 12px;
      font-weight: 500; color: var(--color-muted); cursor: pointer;
      background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 7px;
      white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
      @include phone { padding: 10px; font-size: 13.5px; }
      &:hover { color: var(--color-ink); border-color: var(--color-muted); filter: none; }
      &[aria-checked='true'] { color: var(--color-ink); border-color: var(--color-accent); }
    }
  }
  .about { margin: 6px 2px 0; color: var(--color-muted); @include small(11.5px); min-height: 1.5em; }
  .tools { margin-top: 14px; }
</style>
