<script>
  import { app } from '$lib/state.svelte.js';
  import { styleLabel, styleDescription } from '$lib/format.js';

  function toggle(id) {
    const at = app.chosen.indexOf(id);
    if (at >= 0) app.chosen.splice(at, 1);
    else app.chosen.push(id);
    if (!app.chosen.length) app.chosen = ['original'];
  }
</script>

<div class="grid">
  {#if !app.grid.tiles.length}
    <!-- Ten placeholders while the looks render, so the step has shape from
         the first frame instead of an empty page. -->
    {#each Array(10) as _, i}
      <div class="tile skeleton" style:animation-delay={`${i * 60}ms`}>
        <div class="shim"></div>
        <div class="cap"><b>&nbsp;</b><span>&nbsp;</span></div>
      </div>
    {/each}
  {/if}
  {#each app.grid.tiles as tile (tile.id)}
    <button
      class="tile"
      type="button"
      aria-pressed={app.chosen.includes(tile.id)}
      onclick={() => toggle(tile.id)}
    >
      <img src={tile.image} alt={styleLabel(tile.id)} loading="lazy" />
      <div class="cap">
        <b>{styleLabel(tile.id)}</b>
        <span>{styleDescription(tile.id)}</span>
      </div>
    </button>
  {/each}
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .grid {
    display: grid; gap: 16px;
    /* min() so the track can never be wider than the column it is in:
       at 560px flat, a phone got one tile cropped at both edges. */
    grid-template-columns: repeat(auto-fill, minmax(min(560px, 100%), 1fr));
    @include phone { gap: 12px; }
  }
  .tile {
    border: 1.5px solid var(--color-line); border-radius: var(--radius-panel); overflow: hidden;
    background: var(--color-panel); cursor: pointer; padding: 0; text-align: left;
    color: var(--color-ink); font: inherit; font-weight: 400; transition: border-color .12s;
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-pressed="true"] {
      border-color: var(--color-accent);
      .cap b::after { content: " ✓"; color: var(--color-accent); }
    }
    img { display: block; width: 100%; height: auto; background: #000; }
  }
  .cap {
    padding: 10px 12px; display: flex; flex-direction: column; gap: 2px;
    @include phone { padding: 9px 11px; }
    b { font-weight: 600; font-size: 14px; }
    span {
      color: var(--color-muted); font-size: 12px;
      overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    }
  }

  /* A tile that has not been rendered yet, breathing so the wait reads as
     work rather than as a broken image. */
  .skeleton {
    cursor: default; animation: pulse 1.4s ease-in-out infinite;
    .shim { width: 100%; aspect-ratio: 3 / 2; background: var(--color-panel-2); }
    @include still { animation: none; }
  }
  @keyframes pulse { 0%, 100% { opacity: .55; } 50% { opacity: .95; } }
</style>
