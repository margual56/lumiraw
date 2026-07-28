<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';

  const framing = $derived(app.settings.framing);

  // What the last render measured, when it found something worth offering.
  const hint = $derived(app.framingHint?.proposed ? app.framingHint : null);
  const offersTilt = $derived(!!hint && hint.angle !== 0);
  const offersShift = $derived(!!hint && hint.perspective_v !== 0);

  // Turning an offer down hides that offer, not every future one: a different
  // measurement is a different question and deserves asking again.
  let refused = $state('');
  const offerKey = $derived(hint ? `${hint.angle}|${hint.perspective_v}` : '');
  const offered = $derived(!!hint && refused !== offerKey);

  // Confidence is already the gate on whether anything is offered at all, so a
  // number that got this far is worth showing.
  const unsure = $derived(
    !!hint && Math.max(offersTilt ? hint.angle_confidence : 0,
                       offersShift ? hint.perspective_confidence : 0) < 0.35);

  const offerText = $derived.by(() => {
    if (!hint) return '';
    const angle = `${n(Math.abs(hint.angle), 1)}°`;
    if (offersTilt && offersShift) return t('framing.offer.both', { angle });
    if (offersTilt) return t('framing.offer.tilt', { angle });
    return t('framing.offer.verticals');
  });

  function acceptOffer() {
    // Added to whatever is already set, not written over it.
    if (offersTilt) framing.angle = Math.round((framing.angle + hint.angle) * 100) / 100;
    if (offersShift) {
      const shifted = framing.perspective_v + hint.perspective_v;
      framing.perspective_v = Math.round(Math.min(0.6, Math.max(-0.6, shifted)) * 1000) / 1000;
    }
    refused = offerKey;
  }

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
    refused = '';   // back to square one, so the measurement may speak again
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

    {#if offered}
      <div class="offer" class:unsure>
        <p>{offerText}</p>
        {#if unsure}<p class="thin">{t('framing.offer.unsure')}</p>{/if}
        <div class="actions">
          <button class="accept" onclick={acceptOffer}>{t('framing.offer.accept')}</button>
          <button class="ghost" onclick={() => (refused = offerKey)}>
            {t('framing.offer.refuse')}
          </button>
        </div>
      </div>
    {/if}

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

<style lang="scss">
  /*
   * The same quiet block AutoAmount uses for what the pipeline decided, since
   * this is the same kind of remark.
   */
  .offer {
    margin: -6px 0 16px; padding: 9px 11px;
    border-left: 2px solid var(--color-accent);
    background: var(--color-panel-2); border-radius: 0 6px 6px 0;
    font-size: 12px; color: var(--color-muted);
    p { margin: 0; }
    /* Thin evidence, said in the same place but without the emphasis. */
    &.unsure { border-left-color: var(--color-line); }
  }
  .thin { margin-top: 4px; font-style: italic; font-size: 11.5px; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
  .accept {
    padding: 4px 10px; border: 0; border-radius: 6px; cursor: pointer;
    background: var(--color-accent); color: var(--color-bg);
    font: inherit; font-size: 12px; font-weight: 600;
  }
</style>
