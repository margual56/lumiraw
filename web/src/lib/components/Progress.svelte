<script>
  /** A single bar under the header covering everything the app can be doing. */
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { stageText } from '$lib/format.js';

  const active = $derived(Boolean(app.job) || app.busy);
  // The pipeline reports its own stages now that it runs here, so even a
  // preview gets a real bar instead of a shuttle.
  const reporting = $derived(Boolean(app.job) || Boolean(app.progress));
  const fraction = $derived(app.job ? app.job.progress : (app.progress?.fraction ?? 0));
  const label = $derived.by(() => {
    if (app.job) return `${stageText(app.job.stage, app.job.stage_params)} · ${n(app.job.elapsed, 0)} s`;
    if (app.progress) return stageText(app.progress.code, {});
    return app.busyKey ? t(app.busyKey, app.busyParams) : '';
  });
</script>

<div class="bar" class:active class:determinate={reporting} aria-hidden={!active}>
  <div class="fill" style:width={reporting ? `${(fraction * 100).toFixed(1)}%` : undefined}></div>
  {#if active && label}
    <span class="label">{label}{reporting ? ` · ${n(fraction * 100, 0)} %` : ''}</span>
  {/if}
</div>

<style>
  .bar {
    position: relative; height: 2px; background: var(--line);
    opacity: 0; transition: opacity .18s;
  }
  .bar.active { opacity: 1; }
  .fill { height: 100%; background: var(--accent); width: 0; }
  .bar.determinate .fill { transition: width .3s ease-out; }
  /* Indeterminate: a shuttle, so a long stage still looks alive. */
  .bar:not(.determinate).active .fill {
    width: 34%;
    animation: shuttle 1.15s ease-in-out infinite;
  }
  @keyframes shuttle {
    0%   { margin-left: -34%; }
    100% { margin-left: 100%; }
  }
  .label {
    position: absolute; right: 22px; top: 6px;
    font-size: 11px; color: var(--muted); letter-spacing: .02em;
    white-space: nowrap; pointer-events: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .bar:not(.determinate).active .fill { animation: none; width: 100%; opacity: .4; }
  }
</style>
