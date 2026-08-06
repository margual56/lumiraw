<script>
  import { lightnessPlane, paintMask } from '$lib/zones.js';
  import { GRIP, MIN, cursor, drawn, grip, moved, resized, same } from '$lib/crop.js';
  import Zoom from './Zoom.svelte';
  import Histogram from './Histogram.svelte';
  import { analyse, clipMask } from '$lib/histogram.js';
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';

  /** A preview image with an optional selection rectangle. */
  let {
    src = '',
    rect = $bindable(null),
    onrect = null,         // set instead of binding when the parent needs a say
    selectable = false,    // only the steps that ask for a region can draw one
    guides = false,
    ratio = '',
    natural = null,        // { width, height } of the source, for 'orig' ratio
    transform = '',        // live client-side preview; the overlay ignores it
    mask = null,           // 'shadows' | 'midtones' | 'highlights', to tint
    busy = false,
    zoomable = true,       // offer the 100 % view; not while the frame is uncropped
    inspect = true,        // histogram and clipping overlay
    children = null,       // drawn over the picture, given where it sits: (box) => ...
  } = $props();

  let zoomed = $state(false);

  // The picture's own pixels at its own size, read once per picture, for the
  // histogram and the clipping overlay.
  let stats = $state(null);
  let clipCanvas = null;
  let statsFor = '';

  function readStats() {
    // Only once the element really holds this picture: the effect also runs
    // between a new `src` and its load, when the old pixels are still there.
    if (!img?.naturalWidth || !img.complete || img.getAttribute('src') !== src
        || statsFor === src) return;
    statsFor = src;
    const off = document.createElement('canvas');
    off.width = img.naturalWidth;
    off.height = img.naturalHeight;
    const octx = off.getContext('2d', { willReadFrequently: true });
    octx.drawImage(img, 0, 0);
    try {
      const data = octx.getImageData(0, 0, off.width, off.height);
      stats = analyse(data);
      octx.putImageData(clipMask(data), 0, 0);
      clipCanvas = off;
    } catch {
      stats = null;
      clipCanvas = null;
    }
  }
  let focus = $state([0.5, 0.5]);

  /** Double-click the picture to look at that spot at 100 %. */
  function zoomAt(event) {
    if (!zoomable || !img?.clientWidth) return;
    const box = img.getBoundingClientRect();
    focus = [Math.min(Math.max((event.clientX - box.left) / box.width, 0), 1),
             Math.min(Math.max((event.clientY - box.top) / box.height, 0), 1)];
    zoomed = true;
  }

  let img = $state(null);
  let canvas = $state(null);
  let pending = $state(null);
  let geometry = $state(0);        // bumped on load and resize, to redraw
  // Where the picture sits inside the viewer, for anything drawn over it.
  let placement = $state({ left: 0, top: 0, width: 0, height: 0 });

  // The lightness of the preview, and the buffer the mask is painted into.
  let plane = null;
  let painted = null;
  let planeKey = '';

  function readPlane(w, h) {
    const key = `${src}|${w}x${h}`;
    if (planeKey === key) return plane;
    const off = document.createElement('canvas');
    off.width = w;
    off.height = h;
    const octx = off.getContext('2d', { willReadFrequently: true });
    octx.drawImage(img, 0, 0, w, h);
    try {
      plane = lightnessPlane(octx.getImageData(0, 0, w, h));
    } catch {
      // A preview the canvas will not let us read back is not worth breaking
      // the viewer over; the picture is still there, only the overlay is not.
      plane = null;
    }
    painted = plane ? new ImageData(w, h) : null;
    planeKey = key;
    return plane;
  }

  // The locked aspect, converted for `crop.js`.
  function aspect() {
    if (!ratio || !img?.naturalWidth) return 0;
    const target = ratio === 'orig'
      ? (natural ? natural.width / natural.height : img.naturalWidth / img.naturalHeight)
      : Number(ratio);
    if (!Number.isFinite(target) || target <= 0) return 0;
    return img.naturalWidth / (target * img.naturalHeight);
  }

  function at(event) {
    const box = canvas.getBoundingClientRect();
    return [
      Math.min(Math.max((event.clientX - box.left) / box.width, 0), 1),
      Math.min(Math.max((event.clientY - box.top) / box.height, 0), 1),
    ];
  }

  // The grip in fractions, per axis, since the viewer's box is not square.
  function reach() {
    const box = canvas.getBoundingClientRect();
    return [GRIP / (box.width || 1), GRIP / (box.height || 1)];
  }

  let drag = $state(null);     // { handle, from, was } while the pointer is down
  let hover = $state(null);    // what the pointer is over, for the cursor alone

  function down(event) {
    if (!selectable || !canvas) return;
    const from = at(event);
    // Pressing on the existing box takes hold of it; pressing the picture
    // outside it starts a new one, which is the only thing this used to do.
    drag = { handle: grip(rect, from, reach()), from, was: rect };
    hover = drag.handle;
    pending = null;
    canvas.setPointerCapture(event.pointerId);
  }

  function move(event) {
    const now = at(event);
    if (!drag) {
      hover = selectable ? grip(rect, now, reach()) : null;
      return;
    }
    if (drag.handle === 'move') {
      pending = moved(drag.was, now[0] - drag.from[0], now[1] - drag.from[1]);
    } else if (drag.handle) {
      pending = resized(drag.was, drag.handle, now, aspect());
    } else {
      pending = drawn(drag.from, now, aspect());
    }
  }

  function up() {
    const box = pending;
    const was = drag?.was;
    drag = null;
    pending = null;
    // A press that never moved is a click, and a click is not an edit: in the
    // framing step committing a box is what takes automatic fitting over.
    if (!box || same(box, was)) return;
    // Nor is a 0.1%-of-frame sliver, whether it was swept out by mistake or
    // squeezed out of a handle dragged onto its own anchor.
    if (box[2] < MIN || box[3] < MIN) return;
    if (onrect) onrect(box);
    else rect = box;
  }

  // A cancelled pointer is not a released one.
  function cancel() {
    drag = null;
    pending = null;
  }

  function draw() {
    if (!canvas || !img?.naturalWidth || !img.clientWidth) return;
    const placed = { left: img.offsetLeft, top: img.offsetTop,
                     width: img.clientWidth, height: img.clientHeight };
    if (placed.left !== placement.left || placed.top !== placement.top
        || placed.width !== placement.width || placed.height !== placement.height) placement = placed;
    canvas.style.left = `${img.offsetLeft}px`;
    canvas.style.top = `${img.offsetTop}px`;
    canvas.style.width = `${img.clientWidth}px`;
    canvas.style.height = `${img.clientHeight}px`;
    canvas.width = img.clientWidth;
    canvas.height = img.clientHeight;

    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // The mask goes down first: putImageData writes pixels rather than
    // compositing them, so anything drawn before it would be wiped.
    if (mask && readPlane(canvas.width, canvas.height)) {
      ctx.putImageData(paintMask(plane, mask, painted), 0, 0);
    }
    if (app.showClipping && clipCanvas && !transform) {
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(clipCanvas, 0, 0, canvas.width, canvas.height);
    }

    const box = pending || rect;

    if (guides && !box) {
      thirds(ctx, 0, 0, canvas.width, canvas.height, 'rgba(255,255,255,.22)');
      return;
    }
    if (!box) return;

    const [x, y, w, h] = [box[0] * canvas.width, box[1] * canvas.height,
                          box[2] * canvas.width, box[3] * canvas.height];
    // Dimming everything outside a selection would bury the mask under it, and
    // the two say different things about the same picture.
    if (!mask) {
      ctx.fillStyle = 'rgba(8,9,11,.55)';
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.clearRect(x, y, w, h);
    }
    if (guides) thirds(ctx, x, y, w, h, 'rgba(255,255,255,.28)');
    ctx.strokeStyle = '#e5a03c';
    ctx.lineWidth = 1.5;
    ctx.strokeRect(x + .75, y + .75, w - 1.5, h - 1.5);
    handles(ctx, x, y, w, h);
  }

  /** Eight marks, because there are eight things to take hold of. */
  function handles(ctx, x, y, w, h) {
    ctx.fillStyle = '#e5a03c';
    for (const [cx, cy] of [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]) {
      ctx.fillRect(cx - 3, cy - 3, 6, 6);
    }
    const bar = Math.min(18, w / 3, h / 3);
    if (bar < 8) return;
    ctx.fillRect(x + (w - bar) / 2, y - 1.5, bar, 3);
    ctx.fillRect(x + (w - bar) / 2, y + h - 1.5, bar, 3);
    ctx.fillRect(x - 1.5, y + (h - bar) / 2, 3, bar);
    ctx.fillRect(x + w - 1.5, y + (h - bar) / 2, 3, bar);
  }

  function thirds(ctx, x, y, w, h, colour) {
    ctx.strokeStyle = colour;
    ctx.lineWidth = 1;
    for (let i = 1; i < 3; i++) {
      ctx.beginPath();
      ctx.moveTo(x + (w * i) / 3, y);
      ctx.lineTo(x + (w * i) / 3, y + h);
      ctx.moveTo(x, y + (h * i) / 3);
      ctx.lineTo(x + w, y + (h * i) / 3);
      ctx.stroke();
    }
  }

  $effect(() => {
    src; rect; pending; guides; geometry; mask; app.showClipping;   // dependencies
    readStats();
    draw();
  });

  function key(event) {
    const target = event.target;
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (['INPUT', 'TEXTAREA', 'SELECT'].includes(target?.tagName) && target.type !== 'range') return;
    if (event.key === 'j' || event.key === 'J') app.showClipping = !app.showClipping;
  }

  $effect(() => {
    if (!img) return;
    const observer = new ResizeObserver(() => (geometry += 1));
    observer.observe(img);
    return () => observer.disconnect();
  });
</script>

<svelte:window onkeydown={key} />

<!-- The picture itself ignores the pointer (it must not be dragged out of
     the page), so the double-click that opens the 100 % view is heard here. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="viewer" class:busy ondblclick={(e) => { if (!zoomed) zoomAt(e); }}>
  <!-- The transform is a local preview of a change the server has not rendered yet. -->
  <img bind:this={img} {src} alt="preview" draggable="false"
       style:transform={transform || undefined}
       onload={() => (geometry += 1)} />
  <canvas
    bind:this={canvas}
    class:selectable
    style:cursor={selectable ? cursor(drag ? drag.handle : hover) : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={cancel}
    onpointerleave={() => { if (!drag) hover = null; }}
  ></canvas>
  {#if children && placement.width && !zoomed}
    {@render children(placement)}
  {/if}
  {#if zoomed}
    <Zoom preview={img} {focus} onclose={() => (zoomed = false)} />
  {/if}
  {#if inspect && src && !zoomed}
    <div class="inspect">
      {#if app.showHistogram}<Histogram {stats} />{/if}
      <div class="toggles">
        <button type="button" aria-pressed={app.showHistogram}
                onclick={() => (app.showHistogram = !app.showHistogram)}>{t('hist.toggle')}</button>
        <button type="button" aria-pressed={app.showClipping} title={t('hist.clip.why')}
                onclick={() => (app.showClipping = !app.showClipping)}>{t('hist.clip')}</button>
      </div>
    </div>
  {/if}
  {#if zoomable && src}
    <button type="button" class="zoom-toggle" aria-pressed={zoomed}
            title={t('zoom.hint')}
            onclick={() => { focus = [0.5, 0.5]; zoomed = !zoomed; }}>
      {zoomed ? t('zoom.fit') : '100%'}
    </button>
  {/if}
</div>

<style lang="scss">
  @use '../../styles/photo' as *;

  .viewer {
    position: relative; border-radius: var(--radius-panel); overflow: hidden;
    background: #000; min-height: 260px;
    display: flex; align-items: center; justify-content: center;
    img { @include fitted; @include undraggable; }
    &.busy::after { @include developing; }
  }
  .inspect {
    position: absolute; left: 10px; top: 10px; z-index: 2;
    display: flex; flex-direction: column; gap: 6px; align-items: flex-start;
    .toggles { display: flex; gap: 4px; }
    button {
      padding: 3px 8px; font-size: 11px; min-width: 0; font-weight: 500;
      background: rgba(0, 0, 0, .6); color: #ccc; border: 1px solid rgba(255, 255, 255, .2);
      &[aria-pressed='true'] { color: #fff; border-color: var(--color-accent); }
    }
  }
  .zoom-toggle {
    position: absolute; top: 10px; right: 10px; z-index: 2;
    padding: 4px 10px; font-size: 12px; min-width: 0;
    background: rgba(0, 0, 0, .6); color: #eee; border: 1px solid rgba(255, 255, 255, .25);
    &:hover { background: rgba(0, 0, 0, .8); }
  }
  canvas {
    position: absolute; touch-action: none; pointer-events: none;
    /* The pointer shape is set inline, from what the pointer is actually over,
       so it is not also declared here where the two would disagree. */
    &.selectable { pointer-events: auto; }
  }
</style>
