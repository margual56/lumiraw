<script>
  /** Before/after wipe.  The container hugs the picture so the divider and the
   *  clipped "after" layer line up with it exactly. */
  import { t } from '$lib/i18n.svelte.js';

  let { before = '', after = '', busy = false,
        beforeLabel = null, afterLabel = null,
        // For pictures on a page rather than in the app: what each one shows,
        // and whether it may wait until it is scrolled to.
        beforeAlt = 'before', afterAlt = 'after', lazy = false,
        width = null, height = null } = $props();

  let at = $state(0.5);
  let dragging = false;
  let host = $state(null);

  const clamp = (v) => Math.min(Math.max(v, 0), 1);

  function move(event) {
    const box = host.getBoundingClientRect();
    at = clamp((event.clientX - box.left) / box.width);
  }

  /** The divider from the keyboard, as a slider: arrows move it by a
   *  twentieth, Home and End go to either edge. */
  function key(event) {
    const step = { ArrowLeft: -0.05, ArrowDown: -0.05, ArrowRight: 0.05, ArrowUp: 0.05 }[event.key];
    const to = { Home: 0, End: 1 }[event.key];
    if (step === undefined && to === undefined) return;
    event.preventDefault();
    at = to ?? clamp(at + step);
  }
</script>

<div
  class="compare"
  class:busy
  bind:this={host}
  style:--split={`${(at * 100).toFixed(2)}%`}
  role="slider" tabindex="0"
  aria-label={t('compare.divider')}
  aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(at * 100)}
  onkeydown={key}
  onpointerdown={(e) => { host.setPointerCapture(e.pointerId); dragging = true; move(e); }}
  onpointermove={(e) => dragging && move(e)}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
>
  <img class="before" src={before} alt={beforeAlt} draggable="false"
       loading={lazy ? 'lazy' : undefined} {width} {height} />
  <img class="after" src={after} alt={afterAlt} draggable="false"
       loading={lazy ? 'lazy' : undefined} {width} {height} />
  <span class="tag left">{beforeLabel ?? t('compare.before')}</span>
  <span class="tag right">{afterLabel ?? t('compare.after')}</span>
  <div class="divider"><i></i></div>
</div>

<style lang="scss">
  @use '../../styles/photo' as *;

  .compare {
    position: relative; border-radius: var(--radius-panel); overflow: hidden;
    /* Sideways drags move the divider; up and down still scroll the page, so
       a finger that lands on the picture on a phone is not trapped there. */
    background: #000; user-select: none; touch-action: pan-y; cursor: ew-resize;
    width: fit-content; max-width: 100%; margin: 0 auto;
    /* The wipe is driven by pointer events on the container, so the pictures
       themselves must stay out of the way of the gesture. */
    img { @include undraggable; }
    img.before { @include fitted; }
    img.after {
      position: absolute; inset: 0; width: 100%; height: 100%;
      clip-path: inset(0 0 0 var(--split, 50%));
    }
    &.busy::after { @include developing; }
  }

  .divider {
    position: absolute; top: 0; bottom: 0; left: var(--split, 50%);
    width: 2px; margin-left: -1px; background: rgb(255 255 255 / 85%);
    box-shadow: 0 0 8px rgb(0 0 0 / 60%); pointer-events: none;
    /* The grab handle, with an arrow on each side made of borders. */
    i {
      position: absolute; top: 50%; left: 50%; width: 34px; height: 34px;
      transform: translate(-50%, -50%); border-radius: 50%;
      background: rgb(255 255 255 / 92%); box-shadow: 0 2px 10px rgb(0 0 0 / 50%);
      &::before, &::after {
        content: ""; position: absolute; top: 50%; width: 0; height: 0;
        border: 5px solid transparent; margin-top: -5px;
      }
      &::before { left: 4px; border-right-color: var(--color-panel); }
      &::after { right: 4px; border-left-color: var(--color-panel); }
    }
  }

  .tag {
    position: absolute; top: 12px; padding: 4px 10px; border-radius: 999px;
    background: rgb(8 9 11 / 72%); color: #fff; font-size: 11.5px;
    letter-spacing: .04em; text-transform: uppercase; pointer-events: none;
    &.left { left: 12px; }
    &.right { right: 12px; }
  }
  /* Only a keyboard focus gets a ring; a drag should not leave one behind. */
  .compare:focus { outline: none; }
  .compare:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
</style>
