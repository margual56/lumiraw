<script>
  /** What the automation already did, shown beside the control that adjusts it. */
  import { app } from '$lib/state.svelte.js';
  import { t, n, signed } from '$lib/i18n.svelte.js';

  let { id } = $props();

  const entry = $derived(app.toggles.find((toggle) => toggle.id === id));
  const params = $derived(entry?.detail?.params ?? null);
  const code = $derived(entry?.detail?.code ?? '');

  const amount = $derived.by(() => {
    if (!params || code === 'reason') return '';
    if (id === 'exposure') {
      return params.ev === undefined || params.ev === null
        ? '' : t('auto.ev', { ev: signed(params.ev, 2) });
    }
    const gains = params.gains;
    if (!Array.isArray(gains) || gains.length < 3) return '';
    return t('auto.gains', { r: n(gains[0]), g: n(gains[1]), b: n(gains[2]) });
  });

  // Two different sentences rather than one with a noun slotted in: "measured
  // from automatic" is not English.
  const source = $derived(params?.source ? t(`auto.from.${params.source}`) : '');
</script>

{#if entry && entry.enabled && amount}
  <p class="auto">
    <span class="what">{t('auto.applied')}</span>
    <strong>{amount}</strong>
    {#if source}<span class="source">{source}</span>{/if}
  </p>
{:else if entry && !entry.enabled}
  <p class="auto off">{t('auto.off')}</p>
{/if}

<style lang="scss">
  .auto {
    display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 8px;
    margin: -6px 0 16px; padding: 8px 10px;
    border-left: 2px solid var(--color-accent);
    background: var(--color-panel-2); border-radius: 0 6px 6px 0;
    font-size: 12px; color: var(--color-muted);
    strong {
      color: var(--color-ink); font-weight: 600; font-size: 12.5px;
      font-variant-numeric: tabular-nums;
    }
    /* Switched off: the same block, said quietly. */
    &.off { border-left-color: var(--color-line); font-style: italic; }
  }
  .source { flex-basis: 100%; font-size: 11.5px; }
</style>
