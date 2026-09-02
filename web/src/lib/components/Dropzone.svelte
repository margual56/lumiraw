<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { RAW_ACCEPT } from '$lib/format.js';
  import { filesFromDrop } from '$lib/roll.svelte.js';

  let { onpick } = $props();
  let over = $state(false);
  let input;

  /** Every file, not the first: a roll is dropped all at once, or as the
   *  folder it came off the card in. */
  function take(files) {
    over = false;
    if (files?.length) onpick([...files]);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<label
  class="drop"
  class:over
  ondragenter={(e) => { e.preventDefault(); over = true; }}
  ondragover={(e) => { e.preventDefault(); over = true; }}
  ondragleave={() => (over = false)}
  ondrop={async (e) => { e.preventDefault(); take(await filesFromDrop(e.dataTransfer)); }}
>
  <input
    bind:this={input}
    type="file"
    multiple
    accept={RAW_ACCEPT}
    onchange={(e) => take(e.currentTarget.files)}
  />
  <strong>
    <span class="on-hover">{t('upload.drop')}</span>
    <span class="on-touch">{t('upload.drop.touch')}</span>
  </strong>
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
