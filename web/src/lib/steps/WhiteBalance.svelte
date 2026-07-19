<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import AutoAmount from '$lib/components/AutoAmount.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
</script>

<div class="stage">
  <Viewer src={app.preview.image} bind:rect={app.settings.wb_rect} selectable busy={app.busy} />
  <aside>
    <h2>{t('wb.title')}</h2>
    <p class="lede">{t('wb.lede')}</p>
    <AutoAmount id="white_balance" />
    <Slider label={t('wb.temperature')} bind:value={app.settings.temperature}
            tone="temp" step={0.02} format={(v) => n(v)} />
    <Slider label={t('wb.tint')} bind:value={app.settings.tint}
            tone="tint" step={0.02} format={(v) => n(v)} />
    <button class="ghost" onclick={() => (app.settings.wb_rect = null)}>{t('wb.clear')}</button>
  </aside>
</div>
