<script>
  /** Local filters and healed spots, drawn over the picture and moved by hand. */
  import { app } from '$lib/state.svelte.js';

  let { box, tool = null, selected = $bindable(null), brush = 0.012 } = $props();

  let svg = $state(null);
  let drag = $state(null);   // { list, index, which, item, from } while the pointer is down

  const W = $derived(box.width);
  const H = $derived(box.height);
  const long = $derived(Math.max(box.width, box.height));

  function at(event) {
    const r = svg.getBoundingClientRect();
    return [Math.min(Math.max((event.clientX - r.left) / r.width, 0), 1),
            Math.min(Math.max((event.clientY - r.top) / r.height, 0), 1)];
  }

  /** What is shown for item `i` of `list`: the copy being dragged, or the item. */
  const shown = (list, i) => (drag && drag.list === list && drag.index === i
    ? drag.item : app.settings[list][i]);

  function begin(event, list, index, which) {
    event.stopPropagation();
    selected = { list, index };
    drag = { list, index, which, item: $state.snapshot(app.settings[list][index]),
             from: at(event), was: $state.snapshot(app.settings[list][index]) };
    svg.setPointerCapture(event.pointerId);
  }

  function start(event) {
    const p = at(event);
    if (tool === 'heal') {
      app.settings.spots = [...app.settings.spots, { at: p, r: brush }];
      selected = { list: 'spots', index: app.settings.spots.length - 1 };
      return;
    }
    if (tool === 'linear') {
      drag = { list: 'filters', index: -1, which: 'b', from: p,
               item: { kind: 'linear', a: p, b: p, exposure: -0.8, temperature: 0, saturation: 0 } };
    } else if (tool === 'radial') {
      drag = { list: 'filters', index: -1, which: 'draw', from: p,
               item: { kind: 'radial', centre: p, radius: [0, 0], feather: 0.5, invert: false,
                       exposure: 0.5, temperature: 0, saturation: 0 } };
    } else {
      selected = null;
      return;
    }
    svg.setPointerCapture(event.pointerId);
  }

  function move(event) {
    if (!drag) return;
    const p = at(event);
    const it = drag.item;
    const d = [p[0] - drag.from[0], p[1] - drag.from[1]];
    const shift = (base) => [base[0] + d[0], base[1] + d[1]];
    switch (drag.which) {
      case 'a': it.a = p; break;
      case 'b': it.b = p; break;
      case 'line': it.a = shift(drag.was.a); it.b = shift(drag.was.b); break;
      case 'centre': it.centre = shift(drag.was.centre); break;
      case 'rx': it.radius = [Math.max(0.01, Math.abs(p[0] - it.centre[0])), it.radius[1]]; break;
      case 'ry': it.radius = [it.radius[0], Math.max(0.01, Math.abs(p[1] - it.centre[1]))]; break;
      case 'draw': {
        // Drawn out from the centre, round on screen unless Shift says otherwise.
        const rx = Math.abs(d[0]), ry = Math.abs(d[1]);
        const r = Math.max(rx * W, ry * H);
        it.radius = event.shiftKey ? [rx, ry] : [r / W, r / H];
        break;
      }
      case 'spot': it.at = shift(drag.was.at); break;
    }
  }

  function end() {
    if (!drag) return;
    const { list, index, item } = drag;
    drag = null;
    if (index === -1) {
      // A click rather than a drag: a sensible default rather than nothing.
      if (item.kind === 'linear' && Math.hypot((item.b[0] - item.a[0]) * W, (item.b[1] - item.a[1]) * H) < 8) {
        item.b = [item.a[0], Math.min(1, item.a[1] + 0.3)];
      }
      if (item.kind === 'radial' && item.radius[0] * W < 8) {
        item.radius = [0.15, (0.15 * W) / H];
      }
      app.settings.filters = [...app.settings.filters, item];
      selected = { list, index: app.settings.filters.length - 1 };
      return;
    }
    app.settings[list][index] = item;
  }

  /** A line through `p` at right angles to a→b, long enough to cross the frame. */
  function across(p, a, b) {
    const dx = (b[0] - a[0]) * W, dy = (b[1] - a[1]) * H;
    const len = Math.hypot(dx, dy) || 1;
    const [nx, ny] = [(-dy / len) * 3 * long, (dx / len) * 3 * long];
    return { x1: p[0] * W - nx, y1: p[1] * H - ny, x2: p[0] * W + nx, y2: p[1] * H + ny };
  }

  const isSel = (list, i) => selected?.list === list && selected.index === i;
  const drafting = $derived(drag && drag.index === -1 ? drag.item : null);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg bind:this={svg} class="local" class:drawing={!!tool}
     style:left={`${box.left}px`} style:top={`${box.top}px`}
     style:width={`${W}px`} style:height={`${H}px`}
     viewBox={`0 0 ${W} ${H}`}
     onpointerdown={start} onpointermove={move} onpointerup={end} onpointercancel={() => (drag = null)}>
  {#each app.settings.filters as _, i}
    {@const f = shown('filters', i)}
    <g class:sel={isSel('filters', i)}>
      {#if f.kind === 'linear'}
        <line class="edge" {...across(f.a, f.a, f.b)} />
        <line class="edge dashed" {...across(f.b, f.a, f.b)} />
        <line class="grab" x1={f.a[0] * W} y1={f.a[1] * H} x2={f.b[0] * W} y2={f.b[1] * H}
              onpointerdown={(e) => begin(e, 'filters', i, 'line')} />
        <circle class="handle" cx={f.a[0] * W} cy={f.a[1] * H} r="6"
                onpointerdown={(e) => begin(e, 'filters', i, 'a')} />
        <circle class="handle hollow" cx={f.b[0] * W} cy={f.b[1] * H} r="6"
                onpointerdown={(e) => begin(e, 'filters', i, 'b')} />
      {:else}
        <ellipse class="edge" cx={f.centre[0] * W} cy={f.centre[1] * H}
                 rx={f.radius[0] * W} ry={f.radius[1] * H} />
        <ellipse class="edge dashed" cx={f.centre[0] * W} cy={f.centre[1] * H}
                 rx={f.radius[0] * W * (1 + f.feather)} ry={f.radius[1] * H * (1 + f.feather)} />
        <circle class="handle" cx={f.centre[0] * W} cy={f.centre[1] * H} r="6"
                onpointerdown={(e) => begin(e, 'filters', i, 'centre')} />
        <circle class="handle hollow" cx={(f.centre[0] + f.radius[0]) * W} cy={f.centre[1] * H} r="5"
                onpointerdown={(e) => begin(e, 'filters', i, 'rx')} />
        <circle class="handle hollow" cx={f.centre[0] * W} cy={(f.centre[1] + f.radius[1]) * H} r="5"
                onpointerdown={(e) => begin(e, 'filters', i, 'ry')} />
      {/if}
    </g>
  {/each}
  {#each app.settings.spots as _, i}
    {@const s = shown('spots', i)}
    <circle class="spot" class:sel={isSel('spots', i)} cx={s.at[0] * W} cy={s.at[1] * H}
            r={Math.max(3, s.r * long)} onpointerdown={(e) => begin(e, 'spots', i, 'spot')} />
  {/each}
  {#if drafting}
    {#if drafting.kind === 'linear'}
      <line class="edge sel-line" {...across(drafting.a, drafting.a, drafting.b)} />
      <line class="edge dashed sel-line" {...across(drafting.b, drafting.a, drafting.b)} />
    {:else}
      <ellipse class="edge sel-line" cx={drafting.centre[0] * W} cy={drafting.centre[1] * H}
               rx={drafting.radius[0] * W} ry={drafting.radius[1] * H} />
    {/if}
  {/if}
</svg>

<style lang="scss">
  .local {
    position: absolute; overflow: hidden; touch-action: none; z-index: 1;
    &.drawing { cursor: crosshair; }
  }
  .edge { stroke: rgba(255, 255, 255, .75); stroke-width: 1.5; fill: none; pointer-events: none; }
  .dashed { stroke-dasharray: 5 5; }
  .grab { stroke: transparent; stroke-width: 14; cursor: move; }
  .handle { fill: #fff; stroke: #000; stroke-width: 1; cursor: grab; }
  .hollow { fill: rgba(0, 0, 0, .4); stroke: #fff; stroke-width: 1.5; }
  .sel .edge, .sel-line { stroke: var(--color-accent); }
  .sel .handle { fill: var(--color-accent); }
  .spot {
    fill: rgba(255, 255, 255, .08); stroke: rgba(255, 255, 255, .85); stroke-width: 1.5; cursor: grab;
    &.sel { stroke: var(--color-accent); }
  }
</style>
