<script>
  /** The picture at 100 %: one pixel of the file to one pixel of the screen. */
  import { untrack } from 'svelte';
  import { app, settingsKey } from '$lib/state.svelte.js';
  import * as api from '$lib/api.js';
  import { t } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';

  let { preview = null, focus = [0.5, 0.5], onclose } = $props();

  let canvas = $state(null);
  // Where the view starts; panning owns it from then on.
  let centre = $state(untrack(() => [focus[0], focus[1]]));
  let loading = $state(false);
  let error = $state('');
  let size = $state(0);          // bumped on resize, to redraw
  let drag = null;

  const current = $derived(app.full.key === settingsKey());

  // Develop the full frame once the settings have held still for a moment.
  $effect(() => {
    const key = settingsKey();
    if (app.full.key === key) return;
    const timer = setTimeout(() => develop(key), 500);
    return () => clearTimeout(timer);
  });

  async function develop(key) {
    loading = true;
    error = '';
    try {
      const out = await api.renderFull({ settings: $state.snapshot(app.settings) });
      if (settingsKey() !== key) {
        out.bitmap.close();
        return;
      }
      app.full.bitmap?.close();
      app.full = { key, bitmap: out.bitmap, width: out.width, height: out.height };
    } catch (err) {
      error = errorText(err);
    } finally {
      loading = false;
    }
  }

  /** The source to show, and how many of its pixels fall on one device pixel. */
  function source() {
    if (app.full.bitmap) {
      return { image: app.full.bitmap, w: app.full.width, h: app.full.height, scale: 1 };
    }
    if (preview?.naturalWidth && app.info?.width) {
      const long = Math.max(preview.naturalWidth, preview.naturalHeight);
      return { image: preview, w: preview.naturalWidth, h: preview.naturalHeight,
               scale: long / Math.max(app.info.width, app.info.height) };
    }
    return null;
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const cw = Math.round(canvas.clientWidth * dpr);
    const ch = Math.round(canvas.clientHeight * dpr);
    canvas.width = cw;
    canvas.height = ch;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, cw, ch);
    const src = source();
    if (!src) return;
    // The source pixels the canvas covers, centred on `centre` and held
    // inside the picture's edges.
    const sw = Math.min(src.w, cw * src.scale);
    const sh = Math.min(src.h, ch * src.scale);
    const sx = Math.min(Math.max(centre[0] * src.w - sw / 2, 0), src.w - sw);
    const sy = Math.min(Math.max(centre[1] * src.h - sh / 2, 0), src.h - sh);
    const dw = sw / src.scale;
    const dh = sh / src.scale;
    // Nearest neighbour at 100 %, so what is seen is the pixels themselves.
    ctx.imageSmoothingEnabled = src.scale !== 1;
    ctx.drawImage(src.image, sx, sy, sw, sh, (cw - dw) / 2, (ch - dh) / 2, dw, dh);
  }

  $effect(() => {
    centre; size; app.full.bitmap; preview;
    draw();
  });

  $effect(() => {
    if (!canvas) return;
    const observer = new ResizeObserver(() => (size += 1));
    observer.observe(canvas);
    return () => observer.disconnect();
  });

  function down(event) {
    drag = { x: event.clientX, y: event.clientY, from: [...centre] };
    canvas.setPointerCapture(event.pointerId);
  }

  function move(event) {
    const src = drag && source();
    if (!src) return;
    const k = (window.devicePixelRatio || 1) * src.scale;
    centre = [
      Math.min(Math.max(drag.from[0] - ((event.clientX - drag.x) * k) / src.w, 0), 1),
      Math.min(Math.max(drag.from[1] - ((event.clientY - drag.y) * k) / src.h, 0), 1),
    ];
  }

  function key(event) {
    if (event.key === 'Escape') onclose?.();
  }
</script>

<svelte:window onkeydown={key} />

<div class="zoom">
  <canvas bind:this={canvas}
          onpointerdown={down} onpointermove={move}
          onpointerup={() => (drag = null)} onpointercancel={() => (drag = null)}
          ondblclick={() => onclose?.()}></canvas>
  <p class="state">
    {#if error}{error}
    {:else if !app.full.bitmap}{t('zoom.developing')}
    {:else if loading || !current}{t('zoom.updating')}
    {:else}{t('zoom.actual', { w: app.full.width, h: app.full.height })}{/if}
  </p>
</div>

<style lang="scss">
  .zoom { position: absolute; inset: 0; background: #000; }
  canvas { width: 100%; height: 100%; display: block; cursor: grab; touch-action: none;
           &:active { cursor: grabbing; } }
  .state {
    position: absolute; left: 10px; bottom: 10px; margin: 0; padding: 3px 8px;
    border-radius: 6px; font-size: 12px; color: #ddd; background: rgba(0, 0, 0, .6);
    pointer-events: none; user-select: none;
  }
</style>
