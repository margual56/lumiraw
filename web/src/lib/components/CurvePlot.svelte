<script>
  /** The four curves, drawn from the tables the pipeline will evaluate. */
  let { curve = null, size = 220 } = $props();

  const CHANNELS = [
    { key: 'rgb', colour: 'var(--color-ink)', width: 1.8 },
    { key: 'r', colour: '#e5645a', width: 1.4 },
    { key: 'g', colour: '#6fca8b', width: 1.4 },
    { key: 'b', colour: '#5a9be5', width: 1.4 },
  ];

  /** Whether a table is the diagonal, to within a quarter of a code value. */
  function moves(table) {
    if (!table?.length) return false;
    const last = table.length - 1;
    return table.some((v, i) => Math.abs(v - i / last) > 1 / 1020);
  }

  /** An SVG path through a table, in a box `size` on a side, y downwards. */
  function path(table) {
    const last = table.length - 1;
    // Every second entry: 128 segments across 220 pixels is already more than
    // one per pixel, and the string is half as long.
    const points = [];
    for (let i = 0; i <= last; i += 2) {
      points.push(`${((i / last) * size).toFixed(2)},${((1 - table[i]) * size).toFixed(2)}`);
    }
    const end = `${size},${((1 - table[last]) * size).toFixed(2)}`;
    return `M${points.join('L')}L${end}`;
  }

  const drawn = $derived(
    CHANNELS.map((c) => ({ ...c, table: curve?.[c.key] }))
            .filter((c) => moves(c.table)));
</script>

<svg viewBox={`0 0 ${size} ${size}`} class="plot" aria-hidden="true">
  <rect x="0" y="0" width={size} height={size} class="field" />
  <!--
    Quarters, not a fine grid: the useful reading is which quarter of the range a handle
    is in, and anything denser fights the curve for attention.
  -->
  {#each [1, 2, 3] as i}
    <line x1={(size * i) / 4} y1="0" x2={(size * i) / 4} y2={size} class="grid" />
    <line x1="0" y1={(size * i) / 4} x2={size} y2={(size * i) / 4} class="grid" />
  {/each}
  <line x1="0" y1={size} x2={size} y2="0" class="diagonal" />
  {#each drawn as c (c.key)}
    <path d={path(c.table)} fill="none" stroke={c.colour} stroke-width={c.width}
          stroke-linecap="round" />
  {/each}
</svg>

<style lang="scss">
  .plot {
    display: block; width: 100%; height: auto;
    border: 1px solid var(--color-line); border-radius: 10px;
  }
  .field { fill: var(--color-panel-2); }
  .grid { stroke: var(--color-line); stroke-width: 1; }
  /* What no grade looks like, so a curve can be read against it. */
  .diagonal { stroke: var(--color-line); stroke-width: 1; stroke-dasharray: 3 3; }
</style>
