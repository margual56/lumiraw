<script>
  /** The whole sky from the spot, as if lying on your back with your head
   *  to the north: the zenith in the middle, the horizon round the edge
   *  (east on the left, as it is looking up), the land standing in from it,
   *  and the stars, the moon and the planets at the time picked (lib/drawsky.js).
   *  The panorama's view is marked on it; a click turns the panorama that way. */
  import { t } from '$lib/i18n.svelte.js';
  import { RAD, compassPoint, wrap360 } from '$lib/geo.js';
  import { drawSky } from '$lib/drawsky.js';
  import { landAt } from '$lib/horizon.js';

  /** `sky`: night.js's nightSky, or null by day; `night`: drawSky's limit,
   *  dark, milky and planetName; `tone` the sky's colours `{ top, horizon }`
   *  and `ground` the land's; `path` the sun's for the day, `sun` where it is. */
  let { sky, night, tone, ground, skyline, path, sun, facing = $bindable(), fov } = $props();

  let width = $state(0);
  let height = $state(0);
  let canvas = $state(null);

  const rgb = (c) => `rgb(${c.join(',')})`;
  const SHORT = $derived(t('calc.sun.points').split(','));

  /** The disc: its middle and its radius, room left round it for the compass. */
  const cx = $derived(width / 2);
  const cy = $derived(height / 2);
  const R = $derived(Math.max(10, Math.min(width, height) / 2 - 22));
  const radiusOf = (altitude) => ((90 - altitude) / 90) * R;
  const place = (bearing, altitude) => {
    if (altitude < 0) return null;
    const r = radiusOf(altitude);
    return [cx - r * Math.sin(bearing * RAD), cy - r * Math.cos(bearing * RAD)];
  };

  $effect(() => {
    if (!canvas || !width || !height) return;
    const ratio = devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * ratio)) canvas.width = Math.round(width * ratio);
    if (canvas.height !== Math.round(height * ratio)) canvas.height = Math.round(height * ratio);
    const g = canvas.getContext('2d');
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, width, height);

    g.save();
    g.beginPath();
    g.arc(cx, cy, R, 0, 2 * Math.PI);
    g.clip();
    const air = g.createRadialGradient(cx, cy, 0, cx, cy, R);
    air.addColorStop(0, rgb(tone.top));
    air.addColorStop(1, rgb(tone.horizon));
    g.fillStyle = air;
    g.fillRect(0, 0, width, height);

    if (sky) drawSky(g, sky, { ...night, width, height, place, broken: () => false, scale: R / 90, ranks: 2, sun });

    // The sun's path for the day, and the sun while it is up.
    g.strokeStyle = 'rgba(255,236,190,.55)';
    g.lineWidth = 1.5;
    g.beginPath();
    let drawing = false;
    for (const p of path) {
      const at = place(p.bearing, p.altitude);
      if (at && drawing) g.lineTo(...at);
      else if (at) g.moveTo(...at);
      drawing = !!at;
    }
    g.stroke();
    const up = place(sun.bearing, sun.altitude);
    if (up) {
      g.fillStyle = '#fff4d6';
      g.beginPath();
      g.arc(up[0], up[1], 7, 0, 2 * Math.PI);
      g.fill();
    }

    // The land, from the horizon in to where it stands against the sky.
    if (skyline) {
      g.fillStyle = rgb(ground);
      g.beginPath();
      g.arc(cx, cy, R + 1, 0, 2 * Math.PI);
      for (let b = 0; b <= 360; b += 1) {
        const r = radiusOf(Math.max(0, landAt(skyline, b)));
        const x = cx - r * Math.sin(b * RAD);
        const y = cy - r * Math.cos(b * RAD);
        if (b === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      }
      g.fill('evenodd');
      g.strokeStyle = 'rgba(255,255,255,.22)';
      g.lineWidth = 1;
      g.stroke();
    }
    g.restore();

    // The panorama's view, as on the map.
    if (fov < 360) {
      g.strokeStyle = 'rgba(232,163,61,.85)';
      g.lineWidth = 1.5;
      g.setLineDash([5, 5]);
      g.beginPath();
      for (const b of [facing - fov / 2, facing + fov / 2]) {
        g.moveTo(cx, cy);
        g.lineTo(cx - R * Math.sin(b * RAD), cy - R * Math.cos(b * RAD));
      }
      g.stroke();
      g.setLineDash([]);
    }

    g.strokeStyle = 'rgba(255,255,255,.35)';
    g.lineWidth = 1;
    g.beginPath();
    g.arc(cx, cy, R, 0, 2 * Math.PI);
    g.stroke();
    g.font = '600 12px system-ui, sans-serif';
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillStyle = 'rgba(255,255,255,.75)';
    for (let b = 0; b < 360; b += 45) {
      const r = R + 12;
      g.fillText(compassPoint(b, SHORT), cx - r * Math.sin(b * RAD), cy - r * Math.cos(b * RAD));
    }
    g.textBaseline = 'alphabetic';
  });

  /** A click turns the panorama to look that way. */
  function turn(event) {
    const box = canvas.getBoundingClientRect();
    const x = event.clientX - box.left - cx;
    const y = event.clientY - box.top - cy;
    if (Math.hypot(x, y) > R + 22) return;
    facing = wrap360(Math.atan2(-x, -y) / RAD);
  }
</script>

<div class="dome" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas bind:this={canvas} aria-label={t('calc.sun.night.dome')} onclick={turn}></canvas>
</div>

<style>
  .dome { position: absolute; inset: 0; }
  canvas { display: block; width: 100%; height: 100%; cursor: crosshair; }
</style>
