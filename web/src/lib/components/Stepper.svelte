<script>
  import { app, STEPS, stepVisible } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  let { onstep } = $props();
  let nav = $state(null);

  // On a phone the steps are a horizontal strip, so the one you are on has to
  // be brought into view when it changes.
  const shown = $derived(STEPS.map((label, i) => ({ label, i }))
    .filter(({ i }) => stepVisible(i)));

  $effect(() => {
    const current = nav?.children[shown.findIndex(({ i }) => i === app.step)];
    current?.scrollIntoView({ inline: 'center', block: 'nearest', behavior: 'smooth' });
  });
</script>

<nav bind:this={nav}>
  {#each shown as { label, i }, nth (label)}
    <button
      type="button"
      aria-current={app.step === i}
      disabled={!app.id && i > 0}
      onclick={() => onstep(i)}
    >{nth + 1}. {t(label)}</button>
  {/each}
</nav>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  nav {
    display: flex; gap: 6px; flex-wrap: wrap;
    /* A full-width strip on its own row on a phone, scrolled rather than
       wrapped: eight steps stacked vertically took a third of the screen. */
    @include phone {
      order: 3; width: 100%; flex-wrap: nowrap; gap: 4px;
      overflow-x: auto; overscroll-behavior-x: contain;
      scrollbar-width: none; scroll-snap-type: x proximity;
      &::-webkit-scrollbar { display: none; }
    }
    button {
      background: none; border: 0; color: var(--color-muted);
      font: inherit; font-size: 12.5px; padding: 5px 11px;
      border-radius: 999px; cursor: pointer; white-space: nowrap;
      &:hover:not(:disabled) { color: var(--color-ink); background: var(--color-panel-2); }
      &[aria-current="true"] {
        background: var(--color-accent); color: var(--color-accent-ink); font-weight: 600;
      }
      &:disabled { opacity: .38; cursor: default; }
      @include phone { scroll-snap-align: center; padding: 10px 14px; min-height: 42px; font-size: 14px; }
    }
  }
</style>
