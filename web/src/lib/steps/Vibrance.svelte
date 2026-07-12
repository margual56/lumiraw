<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';
</script>

<div class="stage">
  <Viewer src={app.preview.image} busy={app.busy} />
  <aside>
    <h2>{t('vibrance.title')}</h2>
    <p class="lede">{t('vibrance.lede')}</p>
    <Slider label={t('vibrance.label')} bind:value={app.settings.vibrance}
            tone="vib" step={0.02}
            format={(v) => (v === 0 ? t('vibrance.auto') : signed(v))} />
  </aside>
</div>
