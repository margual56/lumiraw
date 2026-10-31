<script>
  /**
   * Adjusting part of the picture: a graduated filter for a sky, a radial one
   * for a face (or, inverted, for everything around it), and healing for dust
   * and small distractions.
   */
  import GuideLink from '$lib/components/GuideLink.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import LocalOverlay from '$lib/components/LocalOverlay.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed, n } from '$lib/i18n.svelte.js';
  import * as api from '$lib/api.js';

  let tool = $state(null);          // 'linear' | 'radial' | 'heal' | null
  let selected = $state(null);      // { list: 'filters' | 'spots', index }
  let brush = $state(0.012);        // a new spot's radius, as a share of the long edge

  const TOOLS = ['linear', 'radial', 'heal'];
  /** Filters that follow what is in the picture rather than a shape drawn. */
  const MASKS = ['sky', 'subject'];

  function follow(kind) {
    tool = null;
    app.settings.filters = [...app.settings.filters,
      { kind, invert: false, exposure: kind === 'sky' ? -0.5 : 0.3, temperature: 0, saturation: 0 }];
    selected = { list: 'filters', index: app.settings.filters.length - 1 };
  }

  /** The selected mask filter's reach, tinted, to lay over the picture. */
  let overlay = $state({ key: '', url: '', failed: false });
  const maskItem = $derived(item && MASKS.includes(item.kind) ? item : null);
  $effect(() => {
    if (!maskItem || !app.id) return;
    const kind = maskItem.kind;
    const invert = maskItem.invert;
    const key = `${app.id}|${kind}|${invert}|${JSON.stringify(app.settings.framing)}`;
    if (overlay.key === key) return;
    overlay = { key, url: '', failed: false };
    api.mask($state.snapshot(app.settings), kind).then(({ pixels, width, height }) => {
      const grey = new Uint8ClampedArray(pixels);
      const tint = new ImageData(width, height);
      for (let i = 0; i < grey.length; i += 4) {
        const v = invert ? 255 - grey[i] : grey[i];
        tint.data.set([229, 160, 60, Math.round(v * 0.55)], i);
      }
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      canvas.getContext('2d').putImageData(tint, 0, 0);
      if (overlay.key === key) overlay = { key, url: canvas.toDataURL(), failed: false };
    }).catch(() => { if (overlay.key === key) overlay = { key, url: '', failed: true }; });
  });
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
      {#if maskItem && overlay.url}
        <img class="mask" src={overlay.url} alt="" draggable="false"
             style:left={`${box.left}px`} style:top={`${box.top}px`}
             style:width={`${box.width}px`} style:height={`${box.height}px`} />
      {/if}
      <LocalOverlay {box} {tool} {brush} bind:selected />
    {/snippet}
  </Viewer>
  <aside>
    <h2>{t('local.title')}<GuideLink /></h2>
    <p class="lede">{t('local.lede')}</p>

    <div class="tools" role="radiogroup" aria-label={t('local.title')}>
      {#each TOOLS as id}
        <button type="button" role="radio" aria-checked={tool === id}
                onclick={() => (tool = tool === id ? null : id)}>{t(`local.tool.${id}`)}</button>
      {/each}
    </div>
    <p class="hint">{tool ? t(`local.how.${tool}`) : t('local.how.none')}</p>

    <div class="tools follow">
      <span>{t('local.follow')}</span>
      {#each MASKS as kind}
        <button type="button" onclick={() => follow(kind)}>+ {t(`local.tool.${kind}`)}</button>
      {/each}
    </div>
    {#if app.settings.filters.some((f) => MASKS.includes(f.kind))}
      <div class="chips">
        {#each app.settings.filters as f, i}
          {#if MASKS.includes(f.kind)}
            <button type="button" aria-pressed={selected?.list === 'filters' && selected.index === i}
                    onclick={() => { tool = null; selected = { list: 'filters', index: i }; }}>
              {t(`local.tool.${f.kind}`)}{f.invert ? ` (${t('local.inverted')})` : ''}
            </button>
          {/if}
        {/each}
      </div>
    {/if}

    {#if tool === 'heal' && selected?.list !== 'spots'}
      <Slider label={t('local.size')} bind:value={brush} min={0.003} max={0.06} step={0.001}
              format={(v) => `${n(v * 100, 1)} %`} />
    {/if}

    {#if item && selected.list === 'filters'}
      <h3>{t(`local.tool.${item.kind}`)}</h3>
      <Slider label={t('local.exposure')} bind:value={item.exposure} min={-3} max={3} step={0.05} format={ev} />
      <Slider label={t('local.temperature')} bind:value={item.temperature} step={0.02} format={off} />
      <Slider label={t('local.saturation')} bind:value={item.saturation} step={0.02} format={off} />
      {#if maskItem}
        <p class="hint">{overlay.failed ? t('local.mask.failed') : t(`local.mask.${item.kind}`)}</p>
        <label class="check"><input type="checkbox" bind:checked={item.invert} /> {t('local.invert.mask')}</label>
      {/if}
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
  .follow {
    align-items: center;
    span { @include small(12px); color: var(--color-muted); white-space: nowrap; }
  }
  .chips {
    display: flex; flex-wrap: wrap; gap: 4px; margin: 0 0 8px;
    button {
      padding: 4px 10px; font-size: 12px; font-weight: 500; min-width: 0;
      background: var(--color-panel-2); color: var(--color-muted); border: 1px solid var(--color-line);
      &[aria-pressed='true'] { color: var(--color-ink); border-color: var(--color-accent); }
    }
  }
  .mask { position: absolute; pointer-events: none; z-index: 1; }
  .hint { @include small(12px); color: var(--color-muted); margin: 4px 0 10px; }
  .check { display: flex; gap: 8px; align-items: center; font-size: 13px; margin: 4px 0 12px;
           input { accent-color: var(--color-accent); } }
</style>
