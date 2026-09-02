<script>
  /**
   * Adjusting part of the picture: a graduated filter for a sky, a radial one
   * for a face (or, inverted, for everything around it), and healing for dust
   * and small distractions.
   */
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import LocalOverlay from '$lib/components/LocalOverlay.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed, n } from '$lib/i18n.svelte.js';

  let tool = $state(null);          // 'linear' | 'radial' | 'heal' | null
  let selected = $state(null);      // { list: 'filters' | 'spots', index }
  let brush = $state(0.012);        // a new spot's radius, as a share of the long edge

  const TOOLS = ['linear', 'radial', 'heal'];
  const item = $derived(selected ? app.settings[selected.list][selected.index] : null);

  function remove() {
    const { list, index } = selected;
    app.settings[list] = app.settings[list].filter((_, i) => i !== index);
    selected = null;
  }

  function clearAll() {
    app.settings.filters = [];
    app.settings.spots = [];
    selected = null;
  }

  const ev = (v) => (v === 0 ? '0' : `${signed(v, 2)} EV`);
  const off = (v) => (v === 0 ? '0' : signed(v, 2));
  const count = $derived(app.settings.filters.length + app.settings.spots.length);
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy}>
    {#snippet children(box)}
      <LocalOverlay {box} {tool} {brush} bind:selected />
    {/snippet}
  </Viewer>
  <aside>
    <h2>{t('local.title')}</h2>
    <p class="lede">{t('local.lede')}</p>

    <div class="tools" role="radiogroup" aria-label={t('local.title')}>
      {#each TOOLS as id}
        <button type="button" role="radio" aria-checked={tool === id}
                onclick={() => (tool = tool === id ? null : id)}>{t(`local.tool.${id}`)}</button>
      {/each}
    </div>
    <p class="hint">{tool ? t(`local.how.${tool}`) : t('local.how.none')}</p>

    {#if tool === 'heal' && selected?.list !== 'spots'}
      <Slider label={t('local.size')} bind:value={brush} min={0.003} max={0.06} step={0.001}
              format={(v) => `${n(v * 100, 1)} %`} />
    {/if}

    {#if item && selected.list === 'filters'}
      <h3>{t(`local.tool.${item.kind}`)}</h3>
      <Slider label={t('local.exposure')} bind:value={item.exposure} min={-3} max={3} step={0.05} format={ev} />
      <Slider label={t('local.temperature')} bind:value={item.temperature} step={0.02} format={off} />
      <Slider label={t('local.saturation')} bind:value={item.saturation} step={0.02} format={off} />
      {#if item.kind === 'radial'}
        <Slider label={t('local.feather')} bind:value={item.feather} min={0} max={1} step={0.05}
                format={(v) => `${Math.round(v * 100)} %`} />
        <label class="check"><input type="checkbox" bind:checked={item.invert} /> {t('local.invert')}</label>
      {/if}
      <button class="ghost" onclick={remove}>{t('local.remove')}</button>
    {:else if item && selected.list === 'spots'}
      <h3>{t('local.tool.heal')}</h3>
      <Slider label={t('local.size')} bind:value={item.r} min={0.003} max={0.06} step={0.001}
              format={(v) => `${n(v * 100, 1)} %`} />
      <button class="ghost" onclick={remove}>{t('local.remove')}</button>
    {/if}

    {#if count}
      <p class="hint">{t('local.count', { filters: app.settings.filters.length, spots: app.settings.spots.length })}</p>
      <button class="ghost" onclick={clearAll}>{t('local.clear')}</button>
    {/if}
  </aside>
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .tools {
    display: flex; gap: 6px; margin: 4px 0 8px;
    button {
      flex: 1; padding: 6px 4px; font-size: 12.5px; font-weight: 500;
      background: var(--color-panel-2); color: var(--color-ink); border: 1px solid var(--color-line);
      &[aria-checked='true'] { border-color: var(--color-accent); color: var(--color-accent); }
    }
  }
  h3 {
    margin: 16px 0 6px; @include section-label;
  }
  .hint { @include small(12px); color: var(--color-muted); margin: 4px 0 10px; }
  .check { display: flex; gap: 8px; align-items: center; font-size: 13px; margin: 4px 0 12px;
           input { accent-color: var(--color-accent); } }
</style>
