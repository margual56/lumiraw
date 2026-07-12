<script>
  import Compare from '$lib/components/Compare.svelte';
  import Toggles from '$lib/components/Toggles.svelte';
  import CaptureProfile from '$lib/components/CaptureProfile.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
</script>

<div class="stage">
  <Compare before={app.wipe.before} after={app.wipe.after} busy={app.busy} />
  <aside>
    <h2>{t('compare.title')}</h2>
    <p class="lede">{@html t('compare.lede')}</p>
    <Toggles />
    <CaptureProfile />
  </aside>
</div>
