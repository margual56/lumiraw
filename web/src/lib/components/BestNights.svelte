<script>
  /** The coming month's nights at the spot, as a calendar shaded by how good
   *  each is (bestNights in lib/night.js), with the three best spelled out. A
   *  click goes to that night, fully dark, facing the Milky Way's core. */
  import { compassPoint } from '$lib/geo.js';
  import { i18n, n, t } from '$lib/i18n.svelte.js';
  import { topNights } from '$lib/night.js';

  /** `day`: the one picked; `onpick(night)`: one chosen. */
  let { nights, day, onpick } = $props();

  const dateOf = (night) => new Date(`${night.day}T12:00`);
  const best = $derived(topNights(nights));
  /** Blank cells before the first night, the week starting on Monday. */
  const blanks = $derived((dateOf(nights[0]).getDay() + 6) % 7);
  const POINTS = $derived(t('calc.sun.points').split(','));
  /** The weekdays' initials, from a Monday. */
  const WEEK = $derived(Array.from({ length: 7 }, (_, i) =>
    new Date(2024, 0, 1 + i).toLocaleDateString(i18n.locale, { weekday: 'narrow' })));

  function facts(night) {
    return [
      t('calc.sun.nights.hours', { hours: n(night.moonless, 1) }),
      night.core && t('calc.sun.nights.core', { altitude: n(night.core.altitude, 0), side: compassPoint(night.core.bearing, POINTS) }),
      night.cloud != null && t('calc.sun.nights.cloud', { cloud: Math.round(night.cloud) }),
    ].filter(Boolean).join(' · ');
  }
  const longDate = $derived(new Intl.DateTimeFormat(i18n.locale, { weekday: 'short', day: 'numeric', month: 'short' }));
  const long = (night) => longDate.format(dateOf(night));
</script>

<section class="nights">
  {#if best.length}
    <div class="grid">
      {#each WEEK as letter, i (i)}<span class="week">{letter}</span>{/each}
      {#each { length: blanks } as _, i (i)}<span></span>{/each}
      {#each nights as night (night.day)}
        {@const label = `${long(night)}: ${facts(night)}`}
        <button type="button" class:on={night.day === day} class:best={best.includes(night)} style:--good={night.score / best[0].score}
                title={label} aria-label={label} onclick={() => onpick(night)}>
          {dateOf(night).getDate()}
        </button>
      {/each}
    </div>
    <ol>
      {#each best as night (night.day)}
        <li><button type="button" class="link" onclick={() => onpick(night)}>{long(night)}</button> {facts(night)}</li>
      {/each}
    </ol>
    <p class="key">{t('calc.sun.nights.key')}</p>
  {:else}
    <p class="key">{t('calc.sun.nights.none')}</p>
  {/if}
</section>

<style lang="scss">
  .grid { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 3px; max-width: 380px; }
  .week { font-size: 11px; text-align: center; color: var(--color-muted); }
  .grid button {
    min-height: 32px; padding: 0; border: 0; border-radius: 5px; font-size: 12px; font-weight: 500;
    color: var(--color-ink); box-shadow: none; cursor: pointer;
    background: color-mix(in srgb, var(--color-accent) calc(var(--good) * 70%), var(--color-panel-2));
    &.best { font-weight: 700; }
    &.on { outline: 2px solid var(--color-ink); outline-offset: 1px; }
  }
  ol { margin: 10px 0 0; padding: 0; list-style: none; font-size: 13px; line-height: 1.7; }
  .link {
    padding: 0; border: 0; background: none; box-shadow: none; min-height: 0; font: inherit; font-weight: 600;
    color: var(--color-accent); cursor: pointer; text-decoration: underline;
  }
  .key { margin: 6px 0 0; font-size: 12px; color: var(--color-muted); }
</style>
