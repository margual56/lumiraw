<script>
  /** The picture at 100 %: one pixel of the file to one pixel of the screen. */
  import { untrack } from 'svelte';
  import { app, settingsKey } from '$lib/state.svelte.js';
  import * as api from '$lib/api.js';
  import { t } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';

  let { preview = null, focus = [0.5, 0.5], onclose } = $props();

  // How much more than the view a piece covers, so a short drag stays inside
  // it; and the most a piece may be on a side, whatever the screen.
  const SLACK = 1.5;
  const MOST = 3000;

  let canvas = $state(null);
  // Where the view is centred, in fractions of the framed picture.
  let centre = $state(untrack(() => [focus[0], focus[1]]));
  let loading = $state(false);
  let error = $state('');
  let size = $state(0);          // bumped on resize, to redraw
  let drag = null;

  const piece = $derived(app.full);
  const current = $derived(piece.key === settingsKey());
  // Whether what is on screen is all the piece: dragged past its edge, part
  // of the view is the enlarged preview until the next one arrives.
  const whole = $derived.by(() => {
    centre; size; piece.bitmap;
    return covered();
  });

  /** The framed picture's size at full resolution: exact once a piece has
   *  come back, estimated from the preview's shape until then. */
  function frameSize() {
    if (piece.fullWidth) return [piece.fullWidth, piece.fullHeight];
    if (preview?.naturalWidth && app.info?.width) {
      const long = Math.max(app.info.width, app.info.height);
      const k = long / Math.max(preview.naturalWidth, preview.naturalHeight);
      return [preview.naturalWidth * k, preview.naturalHeight * k];
    }
    return null;
  }

  /** The canvas's size in device pixels, which at 100 % is also how many
   *  pixels of the picture it shows. */
  function viewSize() {
    const dpr = window.devicePixelRatio || 1;
    return [Math.round((canvas?.clientWidth ?? 0) * dpr), Math.round((canvas?.clientHeight ?? 0) * dpr)];
  }

  /** The part of the picture under the view: left, top, width, height. */
  function view() {
    const frame = frameSize();
    if (!frame || !canvas) return null;
    const [fw, fh] = frame;
    const [cw, ch] = viewSize();
    const w = Math.min(cw, fw);
    const h = Math.min(ch, fh);
    return [Math.min(Math.max(centre[0] * fw - w / 2, 0), fw - w),
            Math.min(Math.max(centre[1] * fh - h / 2, 0), fh - h), w, h];
  }

  /** Whether the piece we have covers the view. */
  function covered() {
    const v = view();
    if (!v || !piece.bitmap) return false;
    return v[0] >= piece.x && v[1] >= piece.y
      && v[0] + v[2] <= piece.x + piece.width && v[1] + v[3] <= piece.y + piece.height;
  }

  async function develop() {
    // One at a time: whatever changes meanwhile is looked at when it is done.
    if (loading) return;
    const [cw, ch] = viewSize();
    if (!cw || !ch) return;
    const key = settingsKey();
    loading = true;
    error = '';
    try {
      const out = await api.renderRegion({
        settings: $state.snapshot(app.settings),
        centre: [...centre],
        width: Math.min(Math.round(cw * SLACK), MOST),
        height: Math.min(Math.round(ch * SLACK), MOST),
      });
      app.full.bitmap?.close();
      app.full = { key, bitmap: out.bitmap, x: out.x, y: out.y, width: out.width,
                   height: out.height, fullWidth: out.full_width, fullHeight: out.full_height };
    } catch (err) {
      error = errorText(err);
    } finally {
      loading = false;
    }
    // Whatever moved or changed while that one was being developed.
    if (!error && (settingsKey() !== app.full.key || !covered())) develop();
  }

  // A new piece once the settings have held still for a moment: a slider on
  // its way somewhere should not start one per position.
  $effect(() => {
    const key = settingsKey();
    if (piece.key === key) return;
    const timer = setTimeout(develop, 500);
    return () => clearTimeout(timer);
  });

  // And once a drag has taken the view off the edge of the piece.
  $effect(() => {
    centre; size;
    if (untrack(covered)) return;
    const timer = setTimeout(develop, 250);
    return () => clearTimeout(timer);
  });

  // Closing lets the module drop the full-size frame it keeps for this.
  $effect(() => () => {
    app.full.bitmap?.close();
    app.full = { key: null, bitmap: null, x: 0, y: 0, width: 0, height: 0,
                 fullWidth: 0, fullHeight: 0 };
    api.releaseRegion().catch(() => {});
  });

  function draw() {
    if (!canvas) return;
    const [cw, ch] = viewSize();
    canvas.width = cw;
    canvas.height = ch;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, cw, ch);
    const v = view();
    const frame = frameSize();
    if (!v || !frame) return;
    // Centred when the picture is smaller than the view.
    const ox = (cw - v[2]) / 2;
    const oy = (ch - v[3]) / 2;
    // The preview, enlarged, under everything.
    if (preview?.naturalWidth) {
      const k = preview.naturalWidth / frame[0];
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(preview, v[0] * k, v[1] * k, v[2] * k, v[3] * k, ox, oy, v[2], v[3]);
    }
    // The piece at 100 %, where it overlaps the view: nearest neighbour, so
    // what is seen is the pixels themselves.
    if (piece.bitmap) {
      const x0 = Math.max(v[0], piece.x);
      const y0 = Math.max(v[1], piece.y);
      const x1 = Math.min(v[0] + v[2], piece.x + piece.width);
      const y1 = Math.min(v[1] + v[3], piece.y + piece.height);
      if (x1 > x0 && y1 > y0) {
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(piece.bitmap, x0 - piece.x, y0 - piece.y, x1 - x0, y1 - y0,
                      ox + x0 - v[0], oy + y0 - v[1], x1 - x0, y1 - y0);
      }
    }
  }

  $effect(() => {
    centre; size; piece.bitmap; preview;
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
    const frame = drag && frameSize();
    if (!frame) return;
    const k = window.devicePixelRatio || 1;
    centre = [
      Math.min(Math.max(drag.from[0] - ((event.clientX - drag.x) * k) / frame[0], 0), 1),
      Math.min(Math.max(drag.from[1] - ((event.clientY - drag.y) * k) / frame[1], 0), 1),
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
    {:else if !piece.bitmap}{t('zoom.developing')}
    {:else if loading || !current || !whole}{t('zoom.updating')}
    {:else}{t('zoom.actual', { w: piece.fullWidth, h: piece.fullHeight })}{/if}
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
