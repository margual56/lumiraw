<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { RAW_ACCEPT } from '$lib/format.js';

  let { onpick } = $props();
  let over = $state(false);
  let input;

  function take(file) {
    over = false;
    if (file) onpick(file);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<label
  class="drop"
  class:over
  ondragenter={(e) => { e.preventDefault(); over = true; }}
  ondragover={(e) => { e.preventDefault(); over = true; }}
  ondragleave={() => (over = false)}
  ondrop={(e) => { e.preventDefault(); take(e.dataTransfer.files[0]); }}
>
  <input
    bind:this={input}
    type="file"
    accept={RAW_ACCEPT}
    onchange={(e) => take(e.currentTarget.files[0])}
  />
  <strong>{t('upload.drop')}</strong>
  <span>{t('upload.formats')}</span>
  <em>{app.message}</em>
</label>

<style lang="scss">
  /*
   * Fills whatever the upload step gives it, and never gets so short that the
   * target is hard to hit.
   */
  .drop { height: 100%; min-height: 380px; }
</style>
