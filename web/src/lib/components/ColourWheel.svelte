<script>
  /**
   * One grading wheel: drag the point away from the centre toward a colour,
   * and the slider under it for lightness. The disc is drawn in Oklab, the
   * space the pipeline grades in, so the colour under the point is the one
   * that goes in. Double-click puts the point back in the middle.
   */
  import { oklabToSrgb, hueColour } from '$lib/oklab.js';
  import Slider from '$lib/components/Slider.svelte';
  import { t, signed } from '$lib/i18n.svelte.js';

  /** `value` is `{ hue, amount, lum }`; `onchange` gets a new one. */
  let { label, value, onchange } = $props();

  const SIDE = 120;
  let canvas = $state(null);
  let disc = $state(null);
  let dragging = false;

  // The disc, once: hue round the edge, more colour further out.
  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const image = ctx.createImageData(SIDE, SIDE);
    const r = SIDE / 2;
    for (let y = 0; y < SIDE; y++) {
      for (let x = 0; x < SIDE; x++) {
        const dx = x + 0.5 - r, dy = r - (y + 0.5);
        const d = Math.hypot(dx, dy) / r;
        const i = (y * SIDE + x) * 4;
        if (d > 1) continue;
        const angle = Math.atan2(dy, dx);
        const c = 0.13 * d;
        const [R, G, B] = oklabToSrgb(0.7, Math.cos(angle) * c, Math.sin(angle) * c);
        image.data[i] = R;
        image.data[i + 1] = G;
        image.data[i + 2] = B;
        // A soft edge rather than a jagged one.
        image.data[i + 3] = Math.round(255 * Math.min(1, (1 - d) * r));
      }
    }
    ctx.putImageData(image, 0, 0);
  });

  const puck = $derived.by(() => {
    const a = (value.hue * Math.PI) / 180;
    return { x: 50 + Math.cos(a) * value.amount * 50, y: 50 - Math.sin(a) * value.amount * 50 };
  });

  const round = (v, d) => Math.round(v * d) / d;
  const set = (patch) => onchange({ ...value, ...patch });

  function point(event) {
    const box = disc.getBoundingClientRect();
    const dx = (event.clientX - box.left) / box.width * 2 - 1;
    const dy = 1 - (event.clientY - box.top) / box.height * 2;
    const amount = Math.min(1, Math.hypot(dx, dy));
    const hue = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360;
    set({ hue: round(hue, 10), amount: round(amount, 1000) });
  }

  /** Left and right turn the hue, up and down move the point in and out. */
  function key(event) {
    const turn = { ArrowLeft: 5, ArrowRight: -5 }[event.key];
    const out = { ArrowUp: 0.05, ArrowDown: -0.05 }[event.key];
    if (event.key === 'Home' || event.key === 'Escape') {
      event.preventDefault();
      set({ amount: 0 });
      return;
    }
    if (turn === undefined && out === undefined) return;
    event.preventDefault();
    if (turn !== undefined) set({ hue: (value.hue + turn + 360) % 360 });
    else set({ amount: round(Math.min(1, Math.max(0, value.amount + out)), 1000) });
  }

  const describe = $derived(value.amount < 0.005
    ? t('wheels.neutral')
    : t('wheels.at', { hue: Math.round(value.hue), amount: Math.round(value.amount * 100) }));
  const lum = (v) => (v === 0 ? '0' : signed(v));
</script>

<div class="wheel">
  <span class="name">{label}</span>
  <div class="disc" bind:this={disc} role="slider" tabindex="0"
       aria-label={label} aria-valuemin="0" aria-valuemax="100"
       aria-valuenow={Math.round(value.amount * 100)} aria-valuetext={describe}
       title={t('wheels.how')}
       onkeydown={key}
       ondblclick={() => set({ amount: 0 })}
       onpointerdown={(e) => { disc.setPointerCapture(e.pointerId); dragging = true; point(e); }}
       onpointermove={(e) => dragging && point(e)}
       onpointerup={() => (dragging = false)}
       onpointercancel={() => (dragging = false)}>
    <canvas bind:this={canvas} width={SIDE} height={SIDE}></canvas>
    <i class="cross" aria-hidden="true"></i>
    <i class="puck" aria-hidden="true" style:left={`${puck.x}%`} style:top={`${puck.y}%`}
       style:background={value.amount < 0.005 ? 'transparent' : hueColour(value.hue, 0.13 * value.amount)}></i>
  </div>
  <Slider label={t('wheels.lum')} value={value.lum} min={-1} max={1} step={0.05} format={lum}
          commitOnRelease={true} onpreview={(v) => set({ lum: v })} />
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  .wheel { display: flex; flex-direction: column; align-items: stretch; min-width: 0; }
  .name { @include small(11.5px); color: var(--color-muted); text-align: center; margin-bottom: 4px; }
  .disc {
    position: relative; width: 100%; max-width: 132px; aspect-ratio: 1; margin: 0 auto 2px;
    border-radius: 50%; cursor: crosshair; touch-action: none;
    box-shadow: 0 0 0 1px var(--color-line);
    canvas { width: 100%; height: 100%; display: block; border-radius: 50%; }
    &:focus { outline: none; }
    &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }
  }
  .cross {
    position: absolute; left: 50%; top: 50%; width: 9px; height: 9px; transform: translate(-50%, -50%);
    pointer-events: none;
    background: linear-gradient(rgb(0 0 0 / 45%), rgb(0 0 0 / 45%)) center / 1px 100% no-repeat,
                linear-gradient(rgb(0 0 0 / 45%), rgb(0 0 0 / 45%)) center / 100% 1px no-repeat;
  }
  .puck {
    position: absolute; width: 14px; height: 14px; border-radius: 50%; transform: translate(-50%, -50%);
    border: 2px solid #fff; box-shadow: 0 1px 4px rgb(0 0 0 / 60%); pointer-events: none;
  }
</style>
