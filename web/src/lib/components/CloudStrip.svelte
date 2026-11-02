<script>
  /** The clouds forecast over the spot for the day picked and on to noon the
   *  next, as far as the slider runs (lib/weather.js), hour by hour in three layers, the higher up the thinner they tend to
   *  be: a sky of only high cloud can still light up at sunset, while low
   *  cloud hides it. Each hour's band under them says whether it is day or
   *  night there, and the hour of the time picked on the slider is marked. */
  import { t, clock } from '$lib/i18n.svelte.js';
  import { lightOf, sunPosition } from '$lib/calc.js';
  import { DAYS, WEATHER_PAGE } from '$lib/weather.js';

  /** `hours`: those of weather.js's clouds to show, null while loading;
   *  `failed`: they could not be had; `picked`: the one of them picked, if
   *  any; `far`: over a week ahead. */
  let { lat, lon, hours, failed = false, picked, far = false } = $props();

  const LAYERS = ['high', 'mid', 'low'];
  const list = $derived(hours ?? []);
  /** Each hour's light, at its middle. */
  const light = $derived(list.map((h) => lightOf(sunPosition(new Date(h.at.valueOf() + 1800e3), lat, lon).geometric) ?? 'night'));
</script>

<section class="clouds" aria-labelledby="clouds-title">
  <h3 id="clouds-title">{t('calc.sun.clouds.title')}</h3>
  {#if failed}
    <p class="note">{t('calc.sun.clouds.failed')}</p>
  {:else if !hours}
    <p class="note">{t('calc.sun.clouds.loading')}</p>
  {:else if !list.length}
    <p class="note">{t('calc.sun.clouds.outside', { days: DAYS })}</p>
  {:else}
    <div class="grid" style:--hours={list.length}>
      {#each LAYERS as layer (layer)}
        <span class="layer">{t(`calc.sun.clouds.${layer}`)}</span>
        {#each list as h (h.at.valueOf())}
          <span class="cell" class:picked={h === picked} style:--cover={h[layer] / 100}
                title={t('calc.sun.clouds.hour', { time: clock(h.at), total: h.total, low: h.low, mid: h.mid, high: h.high })}></span>
        {/each}
      {/each}
      <span class="layer"></span>
      {#each list as h, i (h.at.valueOf())}
        <span class="light {light[i]}" class:picked={h === picked}></span>
      {/each}
      <span class="layer"></span>
      {#each list as h, i (h.at.valueOf())}
        <span class="hour">{i % 6 === 0 ? clock(h.at) : ''}</span>
      {/each}
    </div>
    <p class="key">
      <span class="scale" aria-hidden="true"></span> {t('calc.sun.clouds.key')} {far ? t('calc.sun.clouds.far') : ''}
      <a href={WEATHER_PAGE} target="_blank" rel="noopener">{t('calc.sun.clouds.source')}</a>
    </p>
  {/if}
</section>

<style lang="scss">
  .clouds { margin-top: 18px; }
  h3 { margin: 0 0 8px; font-size: 13px; font-weight: 600; }
  .note { margin: 0; font-size: 12.5px; color: var(--color-muted); }
  .grid {
    display: grid; grid-template-columns: auto repeat(var(--hours), minmax(0, 1fr)); gap: 2px; align-items: center;
  }
  .layer { padding-right: 8px; font-size: 11.5px; color: var(--color-muted); white-space: nowrap; }
  .cell {
    height: 18px; border-radius: 3px;
    background: color-mix(in srgb, var(--color-ink) calc(var(--cover) * 85%), var(--color-panel-2));
  }
  .light {
    height: 5px; border-radius: 2px; background: #141a33;
    &.day { background: #c9d8ee; }
    &.golden { background: #f2a33a; }
    &.dusk, &.blue { background: #5b78c4; }
  }
  .picked { outline: 2px solid var(--color-accent); outline-offset: 1px; }
  .hour { font-size: 10.5px; color: var(--color-muted); white-space: nowrap; overflow: visible; }
  .key { margin: 8px 0 0; font-size: 11.5px; color: var(--color-muted); display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .scale {
    display: inline-block; width: 60px; height: 8px; border-radius: 2px;
    background: linear-gradient(to right, var(--color-panel-2), color-mix(in srgb, var(--color-ink) 85%, var(--color-panel-2)));
  }
</style>
