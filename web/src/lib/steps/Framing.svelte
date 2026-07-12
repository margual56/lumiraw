<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';

  const framing = $derived(app.settings.framing);

  // Where the handle is right now.  It leads `framing.angle` during a drag and
  // catches up when the drag ends.
  let liveAngle = $state(0);
  $effect(() => { liveAngle = app.settings.framing.angle; });

  // How far the picture on screen is from where the handle says it should be.
  const spin = $derived(liveAngle - (app.frame.angle ?? 0));

  const RATIOS = [
    ['', 'framing.ratio.free'],
    ['orig', 'framing.ratio.orig'],
    ['1', 'framing.ratio.square'],
    ['1.5', 'framing.ratio.32'],
    ['1.33333', 'framing.ratio.43'],
    ['1.77778', 'framing.ratio.169'],
    ['0.8', 'framing.ratio.45'],
  ];

  // With auto-fit on and no crop of your own, the server trims to the largest
  // rectangle the tilt leaves intact; the overlay shows where that lands.
  const shownCrop = $derived(framing.crop ?? (framing.auto_fit ? app.coverCrop : null));

  function setCrop(rect) {
    framing.crop = rect;
    framing.auto_fit = false;      // you have taken the crop over
  }

  function toggleAutoFit(on) {
    framing.auto_fit = on;
    if (on) framing.crop = null;   // back to the automatic rectangle
  }

  function reset() {
    Object.assign(framing,
                  { angle: 0, perspective_v: 0, perspective_h: 0, crop: null, auto_fit: true });
  }
</script>

<div class="stage">
  <Viewer
    src={app.frame.image}
    rect={shownCrop}
    onrect={setCrop}
    selectable
    guides
    ratio={app.ratio}
    natural={app.info && { width: app.info.width, height: app.info.height }}
    transform={Math.abs(spin) > 0.005 ? `rotate(${spin}deg)` : ''}
    busy={app.busy}
  />
  <aside>
    <h2>{t('framing.title')}</h2>
    <p class="lede">{t('framing.lede')}</p>

    <Slider label={t('framing.tilt')} bind:value={framing.angle}
            min={-45} max={45} step={0.1} format={(v) => `${n(v, 1)}°`}
            commitOnRelease onpreview={(v) => (liveAngle = v)} />
    <Slider label={t('framing.shiftV')} bind:value={framing.perspective_v}
            min={-0.6} max={0.6} format={(v) => n(v)} />
    <Slider label={t('framing.shiftH')} bind:value={framing.perspective_h}
            min={-0.6} max={0.6} format={(v) => n(v)} />

    <div class="field">
      <label for="ratio">{t('framing.ratio')}</label>
      <select id="ratio" bind:value={app.ratio}>
        {#each RATIOS as [value, key]}
          <option {value}>{t(key)}</option>
        {/each}
      </select>
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={framing.auto_fit}
        onchange={(event) => toggleAutoFit(event.currentTarget.checked)}
      />
      <span>
        {t('framing.autoFit')}
        <em>{t('framing.autoFitHint')}</em>
      </span>
    </label>

    <button class="ghost" onclick={reset}>{t('framing.reset')}</button>
  </aside>
</div>
