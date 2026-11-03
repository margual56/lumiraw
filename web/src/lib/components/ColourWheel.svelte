<script>
  /**
   * One grading wheel: drag the point away from the centre toward a colour,
   * and the slider under it for lightness.
   *
   * The disc is drawn in Oklab, the space the pipeline grades in, and at the
   * strength the pipeline pushes: the colour under the point is what a mid
   * grey turns into. The ring round it shows each hue at full colour, to aim
   * by. Distance from the centre is the square root of the amount, so the
   * small amounts most grades use get most of the disc.
   *
   * Grabbing the point moves it from where it is; pressing elsewhere puts it
   * there first. Shift slows the drag down for fine work. Double-click, or
   * the clear button, puts it back in the middle.
   */
  import { oklabToSrgb, hueColour, fitChroma, MAX_PUSH } from '$lib/oklab.js';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, signed } from '$lib/i18n.svelte.js';

  /** `value` is `{ hue, amount, lum }`; `onchange` gets a new one. */
  let { label, value, onchange } = $props();

  /** Canvas pixels per side: twice the most the disc is shown at, for sharp
   *  edges on dense screens. */
  const SIDE = 280;
  /** Where the disc ends and the hue ring begins and ends, in radii. */
  const DISC = 0.84, RING_IN = 0.89;
  /** The lightness the disc and the point are drawn at. */
  const SHOWN_L = 0.7;
  /** How much a drag with Shift held moves the point, against a plain one. */
  const FINE = 0.25;
  /** How near the point, in shown pixels, a press grabs it rather than
   *  putting it somewhere new. */
  const GRAB = 14;

  let canvas = $state(null);
  let disc = $state(null);

  const toRadius = (amount) => Math.sqrt(Math.max(0, amount));
  const toAmount = (radius) => Math.min(1, radius) ** 2;

  // The disc and ring, once.
  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const image = ctx.createImageData(SIDE, SIDE);
    const r = SIDE / 2;
    const edge = (inside) => Math.max(0, Math.min(1, inside * r));
    for (let y = 0; y < SIDE; y++) {
      for (let x = 0; x < SIDE; x++) {
        const dx = x + 0.5 - r, dy = r - (y + 0.5);
        const d = Math.hypot(dx, dy) / r;
        const i = (y * SIDE + x) * 4;
        const angle = Math.atan2(dy, dx);
        let chroma, alpha;
        if (d <= DISC) {
          chroma = MAX_PUSH * toAmount(d / DISC);
          alpha = edge(DISC - d);
        } else if (d >= RING_IN && d <= 1) {
          chroma = fitChroma(SHOWN_L, angle, 0.14);
          alpha = Math.min(edge(d - RING_IN), edge(1 - d));
        } else continue;
        const [R, G, B] = oklabToSrgb(SHOWN_L, Math.cos(angle) * chroma, Math.sin(angle) * chroma);
        image.data[i] = R;
        image.data[i + 1] = G;
        image.data[i + 2] = B;
        image.data[i + 3] = Math.round(255 * alpha);
      }
    }
    ctx.putImageData(image, 0, 0);
  });

  /** The point, in -1..1 of the disc with y up. */
  const spot = $derived.by(() => {
    const a = (value.hue * Math.PI) / 180;
    const r = toRadius(value.amount);
    return { x: Math.cos(a) * r, y: Math.sin(a) * r };
  });
  // As percentages of the whole box, ring included.
  const puck = $derived({ x: 50 + spot.x * DISC * 50, y: 50 - spot.y * DISC * 50 });

  const round = (v, d) => Math.round(v * d) / d;
  const set = (patch) => onchange({ ...value, ...patch });

  /** Put the point at disc coordinates, kept inside the disc. */
  function place(x, y) {
    const r = Math.min(1, Math.hypot(x, y));
    const amount = round(toAmount(r), 1000);
    // Near the middle the angle is noise; keep the hue there was.
    const hue = r < 1e-3 ? value.hue : round(((Math.atan2(y, x) * 180) / Math.PI + 360) % 360, 10);
    set({ hue, amount });
  }

  /** A pointer event as disc coordinates, and the box's pixels per unit. */
  function where(event) {
    const box = disc.getBoundingClientRect();
    const unit = (box.width / 2) * DISC;
    return {
      x: (event.clientX - box.left - box.width / 2) / unit,
      y: (box.top + box.height / 2 - event.clientY) / unit,
      unit,
    };
  }

  let drag = null;

  function down(event) {
    if (event.button !== 0) return;
    disc.setPointerCapture(event.pointerId);
    disc.focus();
    const p = where(event);
    const near = Math.hypot(p.x - spot.x, p.y - spot.y) * p.unit <= GRAB;
    if (!near) place(p.x, p.y);
    // From here every move is relative, so the point never jumps under the
    // pointer and Shift can slow it down part way through.
    const from = near ? { ...spot } : { x: p.x, y: p.y };
    drag = { start: p, from, last: p, at: from };
    app.live = true;
  }

  function move(event) {
    if (!drag) return;
    const p = where(event);
    const scale = event.shiftKey ? FINE : 1;
    // Step from the last position, so pressing or letting go of Shift mid-way
    // changes the speed without a jump.
    drag.at = {
      x: drag.at.x + (p.x - drag.last.x) * scale,
      y: drag.at.y + (p.y - drag.last.y) * scale,
    };
    // Keep the drag's own record inside the disc too, or coming back from
    // past the edge would have a dead stretch.
    const r = Math.hypot(drag.at.x, drag.at.y);
    if (r > 1) drag.at = { x: drag.at.x / r, y: drag.at.y / r };
    drag.last = p;
    place(drag.at.x, drag.at.y);
  }

  function up() {
    drag = null;
    app.live = false;
  }

  /** Left and right turn the hue, up and down move the point in and out;
   *  Shift moves further. */
  function key(event) {
    if (event.key === 'Home' || event.key === 'Escape') {
      event.preventDefault();
      set({ amount: 0 });
      return;
    }
    const big = event.shiftKey;
    const turn = { ArrowLeft: 1, ArrowRight: -1 }[event.key];
    const out = { ArrowUp: 1, ArrowDown: -1 }[event.key];
    if (turn === undefined && out === undefined) return;
    event.preventDefault();
    if (turn !== undefined) {
      set({ hue: round((value.hue + turn * (big ? 10 : 2) + 360) % 360, 10) });
    } else {
      const r = Math.min(1, Math.max(0, toRadius(value.amount) + out * (big ? 0.1 : 0.02)));
      set({ amount: round(toAmount(r), 1000) });
    }
  }

  const touched = $derived(value.amount >= 0.0005 || value.lum !== 0);
  const percent = $derived(round(value.amount * 100, 10));
  const describe = $derived(value.amount < 0.0005
    ? t('wheels.neutral')
    : t('wheels.at', { hue: Math.round(value.hue), amount: percent }));
  const lum = (v) => (v === 0 ? '0' : signed(v));

  /** A typed number, kept in range; anything unreadable is let go. */
  function typed(event, min, max, apply) {
    const v = Number(event.currentTarget.value);
    if (event.currentTarget.value === '' || !Number.isFinite(v)) return;
    apply(Math.min(max, Math.max(min, v)));
  }
</script>

<div class="wheel">
  <div class="head">
    <span class="name">{label}</span>
    <button type="button" class="clear" class:hidden={!touched}
            aria-label={t('wheels.clear', { zone: label })} title={t('wheels.clear', { zone: label })}
            onclick={() => set({ amount: 0, lum: 0 })}>×</button>
  </div>
  <div class="disc" bind:this={disc} role="slider" tabindex="0"
       aria-label={label} aria-valuemin="0" aria-valuemax="100"
       aria-valuenow={percent} aria-valuetext={describe}
       title={t('wheels.how')}
       onkeydown={key}
       ondblclick={() => set({ amount: 0 })}
       onpointerdown={down}
       onpointermove={move}
       onpointerup={up}
       onpointercancel={up}
       onlostpointercapture={up}>
    <canvas bind:this={canvas} width={SIDE} height={SIDE}></canvas>
    <i class="cross" aria-hidden="true"></i>
    <i class="puck" aria-hidden="true" class:neutral={value.amount < 0.0005}
       style:left={`${puck.x}%`} style:top={`${puck.y}%`}
       style:background={hueColour(value.hue, MAX_PUSH * value.amount, SHOWN_L)}></i>
  </div>
  <div class="numbers">
    <label>
      <span class="visually-hidden">{t('wheels.hue')}</span>
      <input type="number" min="0" max="359" step="1" value={Math.round(value.hue)}
             disabled={value.amount < 0.0005}
             onchange={(e) => typed(e, 0, 360, (v) => set({ hue: v % 360 }))} /><span>°</span>
    </label>
    <label>
      <span class="visually-hidden">{t('wheels.amount')}</span>
      <input type="number" min="0" max="100" step="0.1" value={percent}
             onchange={(e) => typed(e, 0, 100, (v) => set({ amount: round(v / 100, 1000) }))} /><span>%</span>
    </label>
  </div>
  <Slider label={t('wheels.lum')} value={value.lum} min={-1} max={1} step={0.05} format={lum}
          oninput={(v) => set({ lum: v })}
          onactive={(on) => (app.live = on)} />
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  .wheel { display: flex; flex-direction: column; align-items: stretch; min-width: 0; }
  .head { position: relative; display: flex; justify-content: center; margin-bottom: 4px; }
  .name { @include small(11.5px); color: var(--color-muted); }
  .clear {
    position: absolute; right: 0; top: 50%; transform: translateY(-50%);
    width: 18px; height: 18px; padding: 0; border: 0; border-radius: 50%;
    background: transparent; color: var(--color-muted); font-size: 14px; line-height: 1;
    cursor: pointer;
    &:hover, &:focus-visible { color: var(--color-ink); background: var(--color-line); }
    &.hidden { visibility: hidden; }
  }
  .disc {
    position: relative; width: 100%; max-width: 140px; aspect-ratio: 1; margin: 0 auto 4px;
    border-radius: 50%; cursor: crosshair; touch-action: none;
    canvas { width: 100%; height: 100%; display: block; }
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
    &.neutral { background: transparent !important; }
  }
  .numbers {
    display: flex; justify-content: center; gap: 8px; margin-bottom: 2px;
    @include small(11.5px); color: var(--color-muted);
    label { display: inline-flex; align-items: baseline; gap: 1px; }
    input {
      width: 3.4em; padding: 1px 2px; text-align: right; font: inherit; color: inherit;
      background: transparent; border: 1px solid transparent; border-radius: 3px;
      -moz-appearance: textfield; appearance: textfield;
      &::-webkit-inner-spin-button, &::-webkit-outer-spin-button { -webkit-appearance: none; margin: 0; }
      &:hover:not(:disabled), &:focus { border-color: var(--color-line); outline: none; }
      &:disabled { opacity: 0.5; }
    }
  }
  .visually-hidden {
    position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap;
  }
</style>
