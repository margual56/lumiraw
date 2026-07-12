<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

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
    accept=".arw,.sr2,.srf,.cr2,.cr3,.nef,.raf,.orf,.rw2,.pef,.dng"
    onchange={(e) => take(e.currentTarget.files[0])}
  />
  <strong>{t('upload.drop')}</strong>
  <span>{t('upload.formats')}</span>
  <em>{app.message}</em>
</label>

<style>
  .drop {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; height: 100%; min-height: 380px; cursor: pointer;
    border: 1.5px dashed var(--line); border-radius: 16px; background: var(--panel);
    transition: border-color .15s, background .15s;
  }
  .drop:hover, .drop.over { border-color: var(--accent); background: var(--panel-2); }
  .drop input { display: none; }
  .drop strong { font-size: 19px; font-weight: 600; }
  .drop span { color: var(--muted); font-size: 13px; }
  .drop em { color: var(--accent); font-style: normal; font-size: 13px; min-height: 20px; }
</style>
