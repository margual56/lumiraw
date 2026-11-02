<script>
  /** What the stars come out as, at 100 % on the sensor: a patch of sky
   *  drawn pixel by pixel for the lens, aperture, pixels and exposure set,
   *  each star a dot spread by the lens and smeared along by the turning sky,
   *  enlarged so the pixels can be seen. Redrawn as anything changes. */
  import { t, n } from '$lib/i18n.svelte.js';

  /** `trail`: how far a star moves over the exposure, in pixels (starTrail in calc.js). */
  let { aperture, pitch, trail } = $props();

  /** Sensor pixels across the patch, and how many screen pixels each takes. */
  const ZOOM = 8;
  let width = $state(0);
  const cols = $derived(Math.max(24, Math.floor(width / ZOOM)));
  const ROWS = 30;

  /** The same sky every time: stars at fixed places, of a spread of brightness and colour. */
  const STARS = (() => {
    let seed = 7;
    const random = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
    return Array.from({ length: 70 }, () => ({
      x: random(), y: random(),
      // Many faint ones, a few bright.
      flux: 0.15 + 3.5 * random() ** 4,
      warmth: random() - 0.5,
    }));
  })();
  /** The way the sky drifts across the frame. */
  const ANGLE = (-18 * Math.PI) / 180;

  let canvas = $state(null);
  $effect(() => {
    if (!canvas || !width) return;
    const w = cols;
    const h = ROWS;
    const light = new Float32Array(w * h * 3);
    // The lens's spread, in pixels: the Airy disc (green light) on the
    // softness every lens and the air add, a star some 2–3 pixels across.
    const airy = (0.42 * 0.55 * aperture) / pitch;
    const sigma = Math.sqrt(airy * airy + 0.95 * 0.95);
    const reach = Math.ceil(sigma * 3);
    const dx = Math.cos(ANGLE);
    const dy = Math.sin(ANGLE);
    const steps = Math.max(1, Math.ceil(trail * 3));
    for (const star of STARS) {
      // Scattered over a patch a little wider than the frame, so trails come in from its edges.
      const x0 = star.x * (w + trail) - trail * 0.5 * (1 + dx);
      const y0 = star.y * h;
      const tint = [1 + star.warmth * 0.35, 1, 1 - star.warmth * 0.35];
      // The same light whatever the exposure, spread along the trail.
      const share = star.flux / steps / (2 * Math.PI * sigma * sigma);
      for (let s = 0; s < steps; s++) {
        const along = steps === 1 ? 0 : (trail * s) / (steps - 1);
        const cx = x0 + along * dx;
        const cy = y0 + along * dy;
        for (let y = Math.max(0, Math.floor(cy - reach)); y <= Math.min(h - 1, Math.ceil(cy + reach)); y++) {
          for (let x = Math.max(0, Math.floor(cx - reach)); x <= Math.min(w - 1, Math.ceil(cx + reach)); x++) {
            // The light falling on the whole pixel, taken at its centre.
            const r2 = (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2;
            const v = share * Math.exp(-r2 / (2 * sigma * sigma));
            const at = (y * w + x) * 3;
            light[at] += v * tint[0];
            light[at + 1] += v * tint[1];
            light[at + 2] += v * tint[2];
          }
        }
      }
    }
    const image = new ImageData(w, h);
    const SKY = [10, 14, 26];
    for (let i = 0; i < w * h; i++) {
      for (let c = 0; c < 3; c++) {
        // Bright stars clip to white, as on the sensor.
        image.data[i * 4 + c] = Math.min(255, SKY[c] + 255 * (1 - Math.exp(-2.2 * light[i * 3 + c])));
      }
      image.data[i * 4 + 3] = 255;
    }
    const small = new OffscreenCanvas(w, h);
    small.getContext('2d').putImageData(image, 0, 0);
    canvas.width = w * ZOOM;
    canvas.height = h * ZOOM;
    const g = canvas.getContext('2d');
    g.imageSmoothingEnabled = false;
    g.drawImage(small, 0, 0, w * ZOOM, h * ZOOM);
  });

</script>

<div class="patch" bind:clientWidth={width}>
  <canvas bind:this={canvas} aria-label={t('calc.stars.preview.label', { pixels: n(trail, 1) })}></canvas>
  <span class="tag">{t('calc.stars.preview.zoom', { zoom: ZOOM * 100 })}</span>
</div>

<style lang="scss">
  .patch {
    position: relative; margin-top: 22px; border-radius: 10px; overflow: hidden; background: #0a0e1a;
    canvas { display: block; width: 100%; height: auto; image-rendering: pixelated; }
    .tag { position: absolute; right: 8px; bottom: 6px; font-size: 11px; color: rgba(255, 255, 255, .6); }
  }
</style>
