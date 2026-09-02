<script>
  /** Eight hue bands, one at a time. */
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  // The colour each chip is painted, matching `mixer.rs`'s anchors.
  const BANDS = [
    { key: 'red', swatch: '#ff0000' },
    { key: 'orange', swatch: '#ff8000' },
    { key: 'yellow', swatch: '#ffff00' },
    { key: 'green', swatch: '#00ff00' },
    { key: 'aqua', swatch: '#00ffff' },
    { key: 'blue', swatch: '#0000ff' },
    { key: 'purple', swatch: '#8000ff' },
    { key: 'magenta', swatch: '#ff00ff' },
  ];
  const KNOBS = [
    { key: 'h', label: 'mixer.hue' },
    { key: 's', label: 'mixer.sat' },
    { key: 'l', label: 'mixer.lum' },
  ];

  let band = $state('red');

  const value = (b, k) => app.settings.mixer?.[b]?.[k] ?? 0;

  /** Whether a band has been touched, so a chip can say so without being
   *  opened. Otherwise an edit made two minutes ago is invisible. */
  const touched = (b) => KNOBS.some((k) => Math.abs(value(b, k.key)) > 1e-4);

  function set(b, k, v) {
    const mixer = { ...(app.settings.mixer ?? {}) };
    mixer[b] = { ...(mixer[b] ?? {}), [k]: v };
    app.settings.mixer = mixer;
  }

  function clear() {
    app.settings.mixer = {};
  }

  const anyTouched = $derived(BANDS.some((b) => touched(b.key)));
  const shown = (v) => (v === 0 ? t('grade.off') : `${v > 0 ? '+' : ''}${Math.round(v * 100)}`);
</script>

<details class="mixer">
  <summary>{t('mixer.title')}</summary>
  <p class="lede small">{t('mixer.lede')}</p>

  <div class="chips" role="radiogroup" aria-label={t('mixer.title')}>
    {#each BANDS as item (item.key)}
      <button type="button" role="radio" aria-checked={band === item.key}
              class:touched={touched(item.key)} style:--swatch={item.swatch}
              title={t(`mixer.band.${item.key}`)} onclick={() => (band = item.key)}>
        <span class="dot"></span>
        <span class="name">{t(`mixer.band.${item.key}`)}</span>
      </button>
    {/each}
  </div>

  {#each KNOBS as knob (knob.key)}
    <Slider label={t(knob.label)} value={value(band, knob.key)}
            min={-1} max={1} step={0.05} format={shown} commitOnRelease={true}
            onpreview={(v) => set(band, knob.key, v)} />
  {/each}

  <button class="ghost" type="button" onclick={clear} disabled={!anyTouched}>
    {t('mixer.reset')}
  </button>
</details>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .mixer { margin-top: 16px; border-top: 1px solid var(--color-line); padding-top: 12px; }
  summary { cursor: pointer; color: var(--color-muted); @include small(12px); }
  .small { @include small(11.5px); margin: 10px 0; }
  .chips {
    display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px; margin-bottom: 12px;
  }
  .chips button {
    display: flex; flex-direction: column; align-items: center; gap: 3px;
    padding: 6px 2px; cursor: pointer; font: inherit; font-size: 10px;
    @include phone { padding: 8px 2px; font-size: 12px; }
    color: var(--color-muted); background: var(--color-panel-2);
    border: 1px solid var(--color-line); border-radius: 7px;
    &:hover { border-color: var(--color-muted); filter: none; }
    &[aria-checked="true"] { border-color: var(--color-accent); color: var(--color-ink); }
    /* A band that has been moved is marked whether or not it is the one open,
       so an edit made earlier is not invisible. */
    &.touched .name::after { content: " •"; color: var(--color-accent); }
  }
  .dot {
    width: 14px; height: 14px; border-radius: 50%; background: var(--swatch);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 35%);
  }
  .name { white-space: nowrap; }
</style>
