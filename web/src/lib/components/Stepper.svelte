<script>
  import { app, STEPS } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  let { onstep } = $props();
  let nav = $state(null);

  // On a phone the steps are a horizontal strip, so the one you are on has to
  // be brought into view when it changes.
  $effect(() => {
    const current = nav?.children[app.step];
    current?.scrollIntoView({ inline: 'center', block: 'nearest', behavior: 'smooth' });
  });
</script>

<nav bind:this={nav}>
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

  @media (max-width: 700px) {
    /* A full-width strip on its own row, scrolled rather than wrapped: eight
       steps stacked vertically took a third of the screen. */
    nav {
      order: 3; width: 100%; flex-wrap: nowrap; gap: 4px;
      overflow-x: auto; overscroll-behavior-x: contain;
      scrollbar-width: none; scroll-snap-type: x proximity;
    }
    nav::-webkit-scrollbar { display: none; }
    nav button { scroll-snap-align: center; padding: 8px 12px; min-height: 0; }
  }
</style>
