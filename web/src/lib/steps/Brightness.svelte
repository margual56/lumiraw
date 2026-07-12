<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';
</script>

<div class="stage">
  <Viewer src={app.preview.image} bind:rect={app.settings.exposure_rect} selectable busy={app.busy} />
  <aside>
    <h2>{t('brightness.title')}</h2>
    <p class="lede">{@html t('brightness.lede')}</p>
    <Slider label={t('brightness.fine')} bind:value={app.settings.exposure_bias}
            min={-3} max={3} step={0.1} format={(v) => `${signed(v, 1)} EV`} />
    <button class="ghost" onclick={() => (app.settings.exposure_rect = null)}>
      {t('brightness.clear')}
    </button>
  </aside>
</div>
