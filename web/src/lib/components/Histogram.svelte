<script>
  /**
   * The picture's tones, red, green and blue over each other, with the share of the
   * frame that is blown or crushed underneath.
   */
  import { t, n } from '$lib/i18n.svelte.js';

  let { stats = null } = $props();
  let canvas = $state(null);

  $effect(() => {
    if (!canvas || !stats) return;
    const w = (canvas.width = 256);
    const h = (canvas.height = 80);
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, w, h);
    // Scaled to the tallest bin short of the two ends, so a frame with a
    // large black border does not flatten everything else to nothing.
    let top = 1;
    for (const bins of [stats.r, stats.g, stats.b]) {
      for (let i = 2; i < 254; i++) top = Math.max(top, bins[i]);
    }
    ctx.globalCompositeOperation = 'lighter';
    for (const [bins, colour] of [[stats.r, '#c23'], [stats.g, '#2a4'], [stats.b, '#35c']]) {
      ctx.fillStyle = colour;
      ctx.beginPath();
      ctx.moveTo(0, h);
      for (let i = 0; i < 256; i++) ctx.lineTo(i, h - Math.min(1, bins[i] / top) * h);
      ctx.lineTo(255, h);
      ctx.fill();
    }
  });

  const pct = (v) => (v > 0 && v < 0.001 ? '<0.1' : n(v * 100, 1));
</script>

{#if stats}
  <div class="histogram">
    <canvas bind:this={canvas}></canvas>
    <div class="ends">
      <span class:warn={stats.crushed > 0.005}>{t('hist.crushed', { pct: pct(stats.crushed) })}</span>
      <span class:warn={stats.blown > 0.005}>{t('hist.blown', { pct: pct(stats.blown) })}</span>
    </div>
  </div>
{/if}

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .histogram {
    width: 200px; padding: 6px 6px 4px; border-radius: 8px;
    background: rgba(0, 0, 0, .65); pointer-events: none; user-select: none;
    canvas { width: 100%; height: 56px; display: block; }
    .ends {
      display: flex; justify-content: space-between; margin-top: 3px;
      font-size: 10.5px; color: #bbb;
      @include phone { font-size: 12px; }
      .warn { color: #ff8a70; }
    }
  }
</style>
