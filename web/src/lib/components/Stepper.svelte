<script>
  import { app, STEPS } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  let { onstep } = $props();
</script>

<nav>
  {#each STEPS as label, i}
    <button
      type="button"
      aria-current={app.step === i}
      disabled={!app.id && i > 0}
      onclick={() => onstep(i)}
    >{i + 1}. {t(label)}</button>
  {/each}
</nav>

<style>
  nav { display: flex; gap: 6px; flex-wrap: wrap; }
  nav button {
    background: none; border: 0; color: var(--muted);
    font: inherit; font-size: 12.5px; padding: 5px 11px;
    border-radius: 999px; cursor: pointer; white-space: nowrap;
  }
  nav button:hover:not(:disabled) { color: var(--ink); background: var(--panel-2); }
  nav button[aria-current="true"] {
    background: var(--accent); color: var(--accent-ink); font-weight: 600;
  }
  nav button:disabled { opacity: .38; cursor: default; }
</style>
