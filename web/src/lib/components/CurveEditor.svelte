<script>
  /** The four-channel curve editor: drag the handles, or move the regions. */
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  const SIZE = 260;
  const TABS = [
    { key: 'regions', colour: 'var(--color-accent)' },
    { key: 'rgb', colour: 'var(--color-ink)' },
    { key: 'r', colour: '#e5645a' },
    { key: 'g', colour: '#6fca8b' },
    { key: 'b', colour: '#5a9be5' },
  ];
  const REGIONS = ['shadows', 'darks', 'lights', 'highlights'];

  let tab = $state('regions');
  let svg = $state(null);
  let dragging = $state(-1);

  const active = $derived(TABS.find((x) => x.key === tab));
  const table = $derived(app.curve?.edited?.[tab] ?? null);

  /** The control points of the channel being edited. */
  const points = $derived.by(() => {
    if (tab === 'regions') return [];
    const own = app.settings.curves[tab];
    return own?.length >= 2 ? own : [[0, 0], [1, 1]];
  });

  function write(next) {
    // Sorted, and the ends pinned: the pipeline would tidy this anyway, but the
    // handles have to stop where the picture stops or dragging feels broken.
    const sorted = [...next].sort((a, b) => a[0] - b[0]);
    sorted[0][0] = 0;
    sorted[sorted.length - 1][0] = 1;
    app.settings.curves = { ...app.settings.curves, [tab]: sorted };
  }

  const clamp = (v) => Math.min(Math.max(v, 0), 1);

  function at(event) {
    const box = svg.getBoundingClientRect();
    return [clamp((event.clientX - box.left) / box.width),
            clamp(1 - (event.clientY - box.top) / box.height)];
  }

  function grab(index, event) {
    dragging = index;
    svg.setPointerCapture(event.pointerId);
    event.stopPropagation();
  }

  /** Put handle `index` at (x, y), as far as its neighbours allow. */
  function place(index, x, y) {
    const next = points.map((p) => [...p]);
    const first = index === 0;
    const last = index === next.length - 1;
    // A point may not pass its neighbours: two points at one x is a vertical
    // step, which has no slope and no meaning.
    const low = first ? 0 : next[index - 1][0] + 0.02;
    const high = last ? 1 : next[index + 1][0] - 0.02;
    next[index] = [first || last ? next[index][0] : clamp(Math.min(Math.max(x, low), high)),
                   clamp(y)];
    write(next);
  }

  function move(event) {
    if (dragging < 0) return;
    const [x, y] = at(event);
    place(dragging, x, y);
  }

  /** The same handles from the keyboard: arrows move one, a hundredth at a
   *  time or a twentieth with Shift, and Delete takes it away. */
  function key(index, event) {
    const step = event.shiftKey ? 0.05 : 0.01;
    const [x, y] = points[index];
    const moves = { ArrowUp: [0, step], ArrowDown: [0, -step],
                    ArrowRight: [step, 0], ArrowLeft: [-step, 0] };
    if (moves[event.key]) {
      event.preventDefault();
      place(index, x + moves[event.key][0], y + moves[event.key][1]);
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      drop(index, event);
    }
  }

  function release(event) {
    if (dragging >= 0 && svg.hasPointerCapture(event.pointerId)) {
      svg.releasePointerCapture(event.pointerId);
    }
    dragging = -1;
  }

  /** A click on the field adds a handle where it landed. */
  function add(event) {
    if (tab === 'regions' || dragging >= 0) return;
    const [x, y] = at(event);
    if (x <= 0.02 || x >= 0.98) return;
    const next = points.map((p) => [...p]);
    next.push([x, y]);
    write(next);
  }

  /** And a second click on a handle takes it away again, if it is not an end. */
  function drop(index, event) {
    event.stopPropagation();
    if (index === 0 || index === points.length - 1 || points.length <= 2) return;
    write(points.filter((_, i) => i !== index));
  }

  function path(values) {
    if (!values?.length) return '';
    const last = values.length - 1;
    const steps = [];
    for (let i = 0; i <= last; i += 2) {
      steps.push(`${((i / last) * SIZE).toFixed(2)},${((1 - values[i]) * SIZE).toFixed(2)}`);
    }
    return `M${steps.join('L')}L${SIZE},${((1 - values[last]) * SIZE).toFixed(2)}`;
  }

  function reset() {
    if (tab === 'regions') {
      app.settings.curves = { ...app.settings.curves, regions: {} };
      return;
    }
    app.settings.curves = { ...app.settings.curves, [tab]: [] };
  }

  const regionValue = (key) => app.settings.curves.regions?.[key] ?? 0;
  function setRegion(key, value) {
    const regions = { ...(app.settings.curves.regions ?? {}), [key]: value };
    app.settings.curves = { ...app.settings.curves, regions };
  }
</script>

<details class="editor">
  <summary>{t('editor.title')}</summary>

  <div class="tabs" role="tablist">
    {#each TABS as item (item.key)}
      <button type="button" role="tab" aria-selected={tab === item.key}
              style:--tint={item.colour} onclick={() => (tab = item.key)}>
        {t(`editor.tab.${item.key}`)}
      </button>
    {/each}
  </div>

  <p class="lede small">{t(`editor.${tab === 'regions' ? 'regions' : 'points'}.help`)}</p>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg bind:this={svg} viewBox={`0 0 ${SIZE} ${SIZE}`} class="field"
       class:grabbable={tab !== 'regions'}
       onpointerdown={add} onpointermove={move} onpointerup={release}
       onpointercancel={release} role="img" aria-label={t('editor.title')}>
    <rect x="0" y="0" width={SIZE} height={SIZE} class="back" />
    {#each [1, 2, 3] as i}
      <line x1={(SIZE * i) / 4} y1="0" x2={(SIZE * i) / 4} y2={SIZE} class="grid" />
      <line x1="0" y1={(SIZE * i) / 4} x2={SIZE} y2={(SIZE * i) / 4} class="grid" />
    {/each}
    <line x1="0" y1={SIZE} x2={SIZE} y2="0" class="diagonal" />
    {#if table}
      <path d={path(table)} fill="none" stroke={active.colour} stroke-width="1.8"
            stroke-linecap="round" />
    {/if}
    {#each points as p, i (i)}
      <circle cx={p[0] * SIZE} cy={(1 - p[1]) * SIZE} r={dragging === i ? 7 : 5}
              class="handle" style:--tint={active.colour}
              role="slider" tabindex="0"
              aria-label={t('editor.point', { n: i + 1 })}
              aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(p[1] * 100)}
              aria-valuetext={t('editor.point.value', { x: Math.round(p[0] * 100), y: Math.round(p[1] * 100) })}
              onpointerdown={(e) => grab(i, e)} ondblclick={(e) => drop(i, e)}
              onkeydown={(e) => key(i, e)} />
    {/each}
  </svg>

  {#if tab === 'regions'}
    <div class="regions">
      {#each REGIONS as key (key)}
        <Slider label={t(`editor.region.${key}`)} value={regionValue(key)}
                min={-1} max={1} step={0.05}
                format={(v) => (v === 0 ? t('grade.off') : `${v > 0 ? '+' : ''}${Math.round(v * 100)}`)}
                onpreview={(v) => setRegion(key, v)} commitOnRelease={true} />
      {/each}
    </div>
  {/if}

  <button class="ghost" type="button" onclick={reset}>{t('editor.reset')}</button>
</details>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .editor { margin-top: 16px; border-top: 1px solid var(--color-line); padding-top: 12px; }
  summary { cursor: pointer; color: var(--color-muted); @include small(12px); }
  .small { @include small(11.5px); margin: 10px 0; }
  .tabs { display: flex; gap: 4px; margin-top: 12px; }
  .tabs button {
    flex: 1; padding: 5px 2px; cursor: pointer; font: inherit; @include small(11.5px);
    @include phone { padding: 9px 2px; min-height: 40px; }
    font-weight: 600; color: var(--color-muted); background: var(--color-panel-2);
    border: 1px solid var(--color-line); border-radius: 7px;
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-selected="true"] { color: var(--tint); border-color: var(--tint); }
  }
  .field {
    display: block; width: 100%; height: auto; touch-action: none;
    border: 1px solid var(--color-line); border-radius: 10px;
    &.grabbable { cursor: crosshair; }
  }
  .back { fill: var(--color-panel-2); }
  .grid { stroke: var(--color-line); stroke-width: 1; }
  .diagonal { stroke: var(--color-line); stroke-width: 1; stroke-dasharray: 3 3; }
  .handle {
    fill: var(--color-panel); stroke: var(--tint); stroke-width: 2; cursor: grab;
    &:hover { fill: var(--tint); }
    /* Reached by Tab: filled, and ringed, so it is clear which one the arrow
       keys will move. */
    &:focus { outline: none; }
    &:focus-visible { fill: var(--tint); stroke: var(--color-ink); stroke-width: 3; }
  }
  .regions { margin-top: 12px; }
</style>
