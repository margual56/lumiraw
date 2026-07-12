<script>
  /** Before/after wipe.  The container hugs the picture so the divider and the
   *  clipped "after" layer line up with it exactly. */
  import { t } from '$lib/i18n.svelte.js';

  let { before = '', after = '', busy = false } = $props();

  let at = $state(0.5);
  let dragging = false;
  let host = $state(null);

  function move(event) {
    const box = host.getBoundingClientRect();
    at = Math.min(Math.max((event.clientX - box.left) / box.width, 0), 1);
  }
</script>

<div
  class="compare"
  class:busy
  bind:this={host}
  style:--split={`${(at * 100).toFixed(2)}%`}
  onpointerdown={(e) => { host.setPointerCapture(e.pointerId); dragging = true; move(e); }}
  onpointermove={(e) => dragging && move(e)}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
>
  <img class="before" src={before} alt="before" draggable="false" />
  <img class="after" src={after} alt="after" draggable="false" />
  <span class="tag left">{t('compare.before')}</span>
  <span class="tag right">{t('compare.after')}</span>
  <div class="divider"><i></i></div>
</div>

<style>
  .compare {
    position: relative; border-radius: var(--radius); overflow: hidden;
    background: #000; user-select: none; touch-action: none; cursor: ew-resize;
    width: fit-content; max-width: 100%; margin: 0 auto;
  }
  /* The wipe is driven by pointer events on the container. */
  .compare img {
    pointer-events: none;
    -webkit-user-drag: none;
    user-select: none;
  }
  .compare img.before {
    display: block; width: auto; height: auto;
    max-width: 100%; max-height: var(--fit);
  }
  .compare img.after {
    position: absolute; inset: 0; width: 100%; height: 100%;
    clip-path: inset(0 0 0 var(--split, 50%));
  }
  .divider {
    position: absolute; top: 0; bottom: 0; left: var(--split, 50%);
    width: 2px; margin-left: -1px; background: rgba(255,255,255,.85);
    box-shadow: 0 0 8px rgba(0,0,0,.6); pointer-events: none;
  }
  .divider i {
    position: absolute; top: 50%; left: 50%; width: 34px; height: 34px;
    transform: translate(-50%, -50%); border-radius: 50%;
    background: rgba(255,255,255,.92); box-shadow: 0 2px 10px rgba(0,0,0,.5);
  }
  .divider i::before, .divider i::after {
    content: ""; position: absolute; top: 50%; width: 0; height: 0;
    border: 5px solid transparent;
  }
  .divider i::before { left: 4px;  border-right-color: #16181c; margin-top: -5px; }
  .divider i::after  { right: 4px; border-left-color: #16181c;  margin-top: -5px; }
  .tag {
    position: absolute; top: 12px; padding: 4px 10px; border-radius: 999px;
    background: rgba(8,9,11,.72); color: #fff; font-size: 11.5px; letter-spacing: .04em;
    text-transform: uppercase; pointer-events: none;
  }
  .tag.left { left: 12px; }
  .tag.right { right: 12px; }
  .compare.busy::after {
    content: ""; position: absolute; inset: 0;
    background: rgba(13,14,16,.45) url("data:image/svg+xml;utf8,\
<svg xmlns='http://www.w3.org/2000/svg' width='38' height='38' viewBox='0 0 38 38' stroke='%23e5a03c'>\
<g fill='none' stroke-width='3'><circle cx='19' cy='19' r='16' stroke-opacity='.25'/>\
<path d='M35 19a16 16 0 0 0-16-16'><animateTransform attributeName='transform' type='rotate' \
from='0 19 19' to='360 19 19' dur='.8s' repeatCount='indefinite'/></path></g></svg>") center/38px no-repeat;
  }
</style>
