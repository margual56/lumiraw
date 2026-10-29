<script>
  /**
   * Colour grading: a wheel each for the shadows, midtones and highlights and
   * one over everything, where the zones meet, and a way to fill them in from
   * a photograph whose colour is the one wanted.
   */
  import ColourWheel from '$lib/components/ColourWheel.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app, blankWheels } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';
  import * as api from '$lib/api.js';
  import { RAW_ACCEPT } from '$lib/format.js';

  const ZONES = ['shadows', 'midtones', 'highlights', 'global'];

  let matching = $state(false);
  let note = $state('');
  let picker = $state(null);

  const wheels = $derived(app.settings.wheels ?? blankWheels());
  const touched = $derived(ZONES.some((z) => wheels[z].amount > 0.005 || wheels[z].lum !== 0)
                           || wheels.balance !== 0 || wheels.blending !== 0.5);

  function set(zone, value) {
    app.settings.wheels = { ...wheels, [zone]: value };
  }

  async function match(event) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!file) return;
    matching = true;
    note = '';
    try {
      const fitted = await api.matchColour(file, $state.snapshot(app.settings));
      app.settings.wheels = { ...fitted, balance: 0, blending: 0.5 };
      note = t('match.done', { file: file.name });
    } catch {
      note = t('match.failed', { file: file.name });
    } finally {
      matching = false;
    }
  }

  const offset = (v) => (v === 0 ? '0' : signed(v));
  const share = (v) => `${Math.round(v * 100)}%`;
</script>

<details class="grading" open={touched || undefined}>
  <summary>{t('wheels.title')}</summary>
  <p class="lede small">{t('wheels.lede')}</p>

  <div class="wheels">
    {#each ZONES as zone (zone)}
      <ColourWheel label={t(`wheels.${zone}`)} value={wheels[zone]} onchange={(v) => set(zone, v)} />
    {/each}
  </div>

  <Slider label={t('wheels.balance')} value={wheels.balance} min={-1} max={1} step={0.05}
          format={offset} commitOnRelease={true}
          onpreview={(v) => (app.settings.wheels = { ...wheels, balance: v })} />
  <Slider label={t('wheels.blending')} value={wheels.blending} min={0} max={1} step={0.05}
          format={share} commitOnRelease={true}
          onpreview={(v) => (app.settings.wheels = { ...wheels, blending: v })} />

  <div class="actions">
    <button type="button" class="ghost" disabled={matching || !app.id}
            onclick={() => picker.click()}>
      {matching ? t('match.busy') : t('match.button')}
    </button>
    <button type="button" class="ghost" disabled={!touched}
            onclick={() => (app.settings.wheels = blankWheels())}>{t('wheels.reset')}</button>
  </div>
  <input bind:this={picker} type="file" hidden
         accept={`image/jpeg,image/png,image/webp,${RAW_ACCEPT}`} onchange={match} />
  <p class="small note">{note || t('match.how')}</p>
</details>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .grading { margin-top: 16px; border-top: 1px solid var(--color-line); padding-top: 12px; }
  summary { cursor: pointer; color: var(--color-muted); @include small(12px); }
  .small { @include small(11.5px); margin: 10px 0; }
  .note { color: var(--color-muted); margin-top: 6px; }
  .wheels {
    display: grid; grid-template-columns: 1fr 1fr; gap: 10px 14px; margin: 8px 0 12px;
  }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px; }
</style>
