<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import AutoAmount from '$lib/components/AutoAmount.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';

  const zones = $derived(app.settings.shadows || app.settings.midtones
                         || app.settings.highlights);

  /** Which zone's reach is being shown over the picture, if any. */
  let showing = $state(null);

  const enter = (zone) => () => (showing = zone);
  const leave = (zone) => () => { if (showing === zone) showing = null; };

  function resetZones() {
    app.settings.shadows = 0;
    app.settings.midtones = 0;
    app.settings.highlights = 0;
  }
</script>

<div class="stage">
  <Viewer src={app.preview.image} bind:rect={app.settings.exposure_rect} selectable
          mask={showing} busy={app.busy} />
  <aside>
    <h2>{t('brightness.title')}</h2>
    <p class="lede">{@html t('brightness.lede')}</p>

    <AutoAmount id="exposure" />
    <Slider label={t('brightness.fine')} bind:value={app.settings.exposure_bias}
            min={-3} max={3} step={0.1} format={(v) => `${signed(v, 1)} EV`} />
    <p class="lede zonehint">{t('brightness.fineHint')}</p>

    <!--
      The three zones share a scale, so their handles line up and the picture's own tonal
      order is easy to read off the panel.
    -->
    <div class="zone"
         onpointerenter={enter('shadows')} onpointerleave={leave('shadows')}
         onfocusin={enter('shadows')} onfocusout={leave('shadows')}>
      <Slider label={t('brightness.shadows')} bind:value={app.settings.shadows}
              min={-1} max={1} step={0.02} format={(v) => signed(v, 2)} />
    </div>
    <div class="zone"
         onpointerenter={enter('midtones')} onpointerleave={leave('midtones')}
         onfocusin={enter('midtones')} onfocusout={leave('midtones')}>
      <Slider label={t('brightness.midtones')} bind:value={app.settings.midtones}
              min={-1} max={1} step={0.02} format={(v) => signed(v, 2)} />
    </div>
    <div class="zone"
         onpointerenter={enter('highlights')} onpointerleave={leave('highlights')}
         onfocusin={enter('highlights')} onfocusout={leave('highlights')}>
      <Slider label={t('brightness.highlights')} bind:value={app.settings.highlights}
              min={-1} max={1} step={0.02} format={(v) => signed(v, 2)} />
    </div>
    <p class="lede zonehint">{t('brightness.zoneMask')}</p>

    <div class="row">
      <button class="ghost" onclick={resetZones} disabled={!zones}>
        {t('brightness.zonesReset')}
      </button>
      <button class="ghost" onclick={() => (app.settings.exposure_rect = null)}>
        {t('brightness.clear')}
      </button>
    </div>
  </aside>
</div>

<style lang="scss">
  .zonehint { margin: -8px 0 14px; }
  /* The wrapper exists for the pointer, not for the layout: it must not add
     a box of its own between the three sliders. */
  .zone { display: contents; }
  /* Two controls where there used to be one: let them wrap rather than
     squeezing the labels, which are long in both languages. */
  .row { display: flex; flex-wrap: wrap; gap: 8px; }
</style>
