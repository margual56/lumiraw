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

<style>
  .grid {
    display: grid; gap: 16px;
    /* min() so the track can never be wider than the column it is in:
       at 560px flat, a phone got one tile cropped at both edges. */
    grid-template-columns: repeat(auto-fill, minmax(min(560px, 100%), 1fr));
  }
  .tile {
    border: 1.5px solid var(--line); border-radius: var(--radius); overflow: hidden;
    background: var(--panel); cursor: pointer; padding: 0; text-align: left;
    color: var(--ink); font: inherit; font-weight: 400; transition: border-color .12s;
  }
  .tile:hover { border-color: var(--muted); filter: none; }
  .tile[aria-pressed="true"] { border-color: var(--accent); }
  .tile img { display: block; width: 100%; height: auto; background: #000; }
  .cap { padding: 10px 12px; display: flex; flex-direction: column; gap: 2px; }
  .cap b { font-weight: 600; font-size: 14px; }
  .cap span {
    color: var(--muted); font-size: 12px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .tile[aria-pressed="true"] .cap b::after { content: " ✓"; color: var(--accent); }

  .skeleton { cursor: default; animation: pulse 1.4s ease-in-out infinite; }
  .skeleton .shim { width: 100%; aspect-ratio: 3 / 2; background: var(--panel-2); }
  @keyframes pulse { 0%, 100% { opacity: .55; } 50% { opacity: .95; } }
  @media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }

  @media (max-width: 700px) {
    .grid { gap: 12px; }
    .cap { padding: 9px 11px; }
  }
</style>
