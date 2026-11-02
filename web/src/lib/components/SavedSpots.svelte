<script>
  /** The spots kept by name, in this browser with the rest of what is typed. */
  import { t } from '$lib/i18n.svelte.js';

  /** `spots`: `{ name, lat, lon }`; `placed`: there is a spot to keep; `onpick(spot)`: one chosen. */
  let { spots = $bindable(), lat, lon, placed, onpick } = $props();

  let name = $state('');
  function save(event) {
    event.preventDefault();
    const kept = name.trim() || `${lat}, ${lon}`;
    spots = [...spots.filter((spot) => spot.name !== kept), { name: kept, lat, lon }];
    name = '';
  }
</script>

{#if spots.length || placed}
  <div class="spots" role="group" aria-label={t('calc.sun.spots')}>
    {#each spots as spot (spot.name)}
      <span class="spot" class:on={spot.lat === lat && spot.lon === lon}>
        <button type="button" class="go" onclick={() => onpick(spot)}>{spot.name}</button>
        <button type="button" class="off" aria-label={t('calc.sun.spots.remove', { name: spot.name })}
                title={t('calc.sun.spots.remove', { name: spot.name })}
                onclick={() => (spots = spots.filter((other) => other !== spot))}>×</button>
      </span>
    {/each}
    {#if placed}
      <form onsubmit={save}>
        <input type="text" bind:value={name} maxlength="40" placeholder={t('calc.sun.spots.name')} aria-label={t('calc.sun.spots.name')} />
        <button type="submit" class="ghost">{t('calc.sun.spots.save')}</button>
      </form>
    {/if}
  </div>
{/if}

<style lang="scss">
  .spots { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin-top: 10px; font-size: 13px; }
  .spot {
    display: inline-flex; align-items: center; border-radius: 999px; background: var(--color-panel-2);
    box-shadow: inset 0 0 0 1px var(--color-line);
    &.on { box-shadow: inset 0 0 0 1px var(--color-accent); }
    button { min-height: 32px; border: 0; background: none; box-shadow: none; color: var(--color-ink); font-size: 13px; cursor: pointer; }
    .go { padding: 4px 4px 4px 12px; }
    .off { padding: 4px 10px 4px 6px; color: var(--color-muted); &:hover { color: var(--color-ink); } }
  }
  form {
    display: flex; gap: 6px; align-items: center;
    input { width: 150px; margin: 0; min-height: 32px; padding: 4px 8px; font-size: 13px; }
    button { min-height: 32px; padding: 4px 10px; font-size: 13px; }
  }
</style>
