<script>
  /** The spot picked on the golden hour's map, from the ground: the nearest
   *  street-level photo on Mapillary, with the sun's path for the day drawn
   *  over it and the light tinted to the hour picked, so it shows whether
   *  the sunrise or the sunset can be seen from there or goes behind
   *  something. Loaded only once a spot is picked. */
  import { t, n, i18n } from '$lib/i18n.svelte.js';
  import { MAPILLARY_TOKEN } from '$lib/brand.js';
  import { sunPosition } from '$lib/calc.js';

  let { lat, lon, day, light } = $props();

  const RAD = Math.PI / 180;
  const GRAPH = 'https://graph.mapillary.com/images';

  let status = $state('looking');
  let distance = $state(0);
  let spherical = $state(false);
  /** Which way the viewer looks: bearing and tilt, and its vertical field of view, in degrees. */
  let view = $state.raw(null);
  let width = $state(0);
  let height = $state(0);
  let minutes = $state(18 * 60);

  let element = $state(null);
  let canvas = $state(null);
  let viewer = null;

  // -- the photo ------------------------------------------------------------------

  /** Metres between two places close together. */
  const metresTo = (lat2, lon2) => Math.hypot((lat2 - lat) * 111320, (lon2 - lon) * 111320 * Math.cos(lat * RAD));

  /** The nearest photo: a panorama within some 200 m, then 1 km, then any photo within 200 m. */
  async function nearest() {
    for (const [reach, pano] of [[0.002, true], [0.009, true], [0.002, false]]) {
      const across = reach / Math.cos(lat * RAD);
      const query = new URLSearchParams({
        access_token: MAPILLARY_TOKEN, fields: 'id,geometry', limit: '100',
        bbox: [lon - across, lat - reach, lon + across, lat + reach].map((x) => x.toFixed(6)).join(','),
      });
      if (pano) query.set('is_pano', 'true');
      const response = await fetch(`${GRAPH}?${query}`);
      if (!response.ok) throw new Error(String(response.status));
      const found = (await response.json()).data
        .map((image) => ({ id: image.id, far: metresTo(image.geometry.coordinates[1], image.geometry.coordinates[0]) }))
        .sort((a, b) => a.far - b.far)[0];
      if (found) return found;
    }
    return null;
  }

  /** Where the viewer looks now, read on every turn of it. */
  let asked = false;
  function look() {
    if (asked || !viewer) return;
    asked = true;
    requestAnimationFrame(async () => {
      const [pov, fov] = await Promise.all([viewer.getPointOfView(), viewer.getFieldOfView()]);
      view = { bearing: pov.bearing, tilt: pov.tilt, fov };
      asked = false;
    });
  }

  let searched = 0;
  $effect(() => {
    if (!element) return;
    const spot = [lat, lon];
    const search = ++searched;
    status = 'looking';
    (async () => {
      try {
        const found = await nearest();
        if (search !== searched) return;
        if (!found) {
          status = 'none';
          return;
        }
        distance = found.far;
        if (viewer) {
          await viewer.moveTo(found.id);
        } else {
          const { Viewer } = await import('mapillary-js');
          await import('mapillary-js/dist/mapillary.css');
          if (search !== searched) return;
          viewer = new Viewer({
            accessToken: MAPILLARY_TOKEN, container: element, imageId: found.id,
            component: { cover: false, bearing: false, zoom: false },
          });
          for (const event of ['pov', 'fov', 'position', 'image']) viewer.on(event, look);
          viewer.on('image', async () => {
            spherical = (await viewer.getImage())?.cameraType === 'spherical';
          });
        }
        status = 'ready';
        look();
      } catch {
        if (search === searched) status = 'failed';
      }
    })();
    void spot;
  });
  $effect(() => () => {
    viewer?.remove();
    viewer = null;
  });

  // -- the sun --------------------------------------------------------------------

  /** A minute of the day picked, as a Date in this device's time zone. */
  const at = (m) => {
    const date = new Date(`${day}T00:00`);
    date.setMinutes(m);
    return date;
  };
  const minuteOf = (date) => date.getHours() * 60 + date.getMinutes();

  // Each new day or place opens on the evening's golden hour, the light most come for.
  $effect(() => {
    const start = light?.eveningGolden?.from ?? light?.sunset ?? light?.noon;
    if (start) minutes = minuteOf(start);
  });

  /** The sun every four minutes of the day, for its path. */
  const path = $derived(Array.from({ length: 361 }, (_, i) => ({ m: i * 4, ...sunPosition(at(i * 4), lat, lon) })));
  const sun = $derived(sunPosition(at(minutes), lat, lon));

  /** Where a direction lands on the view, or null if it is behind it. */
  function project(bearing, altitude) {
    const turn = (bearing - view.bearing) * RAD;
    const up = altitude * RAD;
    const tilt = view.tilt * RAD;
    // The direction against the camera: right, ahead, and up, tilted with it.
    const right = Math.sin(turn) * Math.cos(up);
    const level = Math.cos(turn) * Math.cos(up);
    const ahead = level * Math.cos(tilt) + Math.sin(up) * Math.sin(tilt);
    const above = -level * Math.sin(tilt) + Math.sin(up) * Math.cos(tilt);
    if (ahead < 0.05) return null;
    const scale = height / 2 / Math.tan((view.fov * RAD) / 2);
    return { x: width / 2 + (right / ahead) * scale, y: height / 2 - (above / ahead) * scale };
  }

  /** The light by the sun's altitude: the sky at the top and at the horizon,
   *  how much it tints the photo, and how dark it makes it. */
  const SKY = [
    [-18, [5, 7, 15], [11, 16, 34], 0.9, 0.75],
    [-10, [11, 22, 56], [28, 44, 92], 0.85, 0.55],
    [-6, [29, 50, 112], [61, 90, 160], 0.8, 0.35],
    [-4, [47, 76, 151], [122, 127, 192], 0.75, 0.2],
    [-1, [74, 95, 168], [240, 138, 93], 0.75, 0.08],
    [2, [109, 134, 196], [255, 154, 69], 0.7, 0],
    [6, [143, 176, 224], [255, 192, 112], 0.5, 0],
    [15, [156, 192, 238], [244, 226, 192], 0.2, 0],
    [30, [168, 204, 244], [232, 238, 246], 0, 0],
  ];
  function sky(altitude) {
    const i = SKY.findIndex(([h]) => h > altitude);
    if (i <= 0) {
      const [, top, horizon, tint, dark] = SKY[i === 0 ? 0 : SKY.length - 1];
      return { top, horizon, tint, dark };
    }
    const [h0, top0, horizon0, tint0, dark0] = SKY[i - 1];
    const [h1, top1, horizon1, tint1, dark1] = SKY[i];
    const k = (altitude - h0) / (h1 - h0);
    const mix = (a, b) => a.map((x, j) => Math.round(x + (b[j] - x) * k));
    return { top: mix(top0, top1), horizon: mix(horizon0, horizon1), tint: tint0 + (tint1 - tint0) * k, dark: dark0 + (dark1 - dark0) * k };
  }
  const rgb = (c, alpha = 1) => `rgba(${c.join(',')},${alpha})`;
  const tone = $derived(sky(sun.altitude));

  /** The tint over the photo: the sky's colours down to the horizon, a glow
   *  about the sun while it is low, and the dark of night. */
  const horizonAt = $derived(view && height ? project(view.bearing, 0)?.y ?? height / 2 : 0);
  const glowAt = $derived(view && width ? project(sun.bearing, sun.altitude) : null);
  const tint = $derived(height
    ? `linear-gradient(to bottom, ${rgb(tone.top)} 0, ${rgb(tone.horizon)} ${Math.round(horizonAt)}px, ${rgb(tone.horizon, 0.6)} 100%)`
    : 'none');
  const glow = $derived(glowAt && sun.altitude > -3 && sun.altitude < 12
    ? `radial-gradient(circle at ${Math.round(glowAt.x)}px ${Math.round(glowAt.y)}px, ${rgb(tone.horizon, 0.85)} 0, ${rgb(tone.horizon, 0)} ${Math.round(Math.max(width, height) * 0.45)}px)`
    : 'none');

  /** The day's light along the slider: night, blue, gold, day, and back. */
  const track = $derived(`linear-gradient(to right, ${path.filter((_, i) => i % 6 === 0).map((p) => {
    const s = sky(p.altitude);
    return `${rgb(s.top.map((x, j) => Math.round((x + s.horizon[j]) / 2)))} ${((p.m / 1440) * 100).toFixed(1)}%`;
  }).join(', ')})`);

  const clock = (m) => new Intl.DateTimeFormat(i18n.locale, { hour: '2-digit', minute: '2-digit' }).format(at(m));
  const compass = (bearing) => t('calc.sun.compass').split(',')[Math.round(bearing / 22.5) % 16];

  // -- the drawing ----------------------------------------------------------------

  $effect(() => {
    if (!canvas || !view || !width || !height) return;
    const ratio = devicePixelRatio || 1;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    const g = canvas.getContext('2d');
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, width, height);
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.font = '600 12px system-ui, sans-serif';
    g.shadowColor = 'rgba(0,0,0,.6)';

    // The horizon as it would be with nothing on it, across what is in view.
    g.setLineDash([6, 6]);
    g.strokeStyle = 'rgba(255,255,255,.55)';
    g.lineWidth = 1.5;
    g.beginPath();
    let drawing = false;
    for (let b = -100; b <= 100; b += 2) {
      const p = project(view.bearing + b, 0);
      if (!p) { drawing = false; continue; }
      if (drawing) g.lineTo(p.x, p.y); else g.moveTo(p.x, p.y);
      drawing = true;
    }
    g.stroke();
    g.setLineDash([]);

    // The path, in stretches coloured by the light: day, golden, blue.
    const kind = (altitude) => (altitude > 6 ? 'day' : altitude > -0.833 ? 'golden' : altitude > -6 ? 'blue' : null);
    const STYLE = { day: ['rgba(255,244,214,.75)', 2], golden: ['#f2a33a', 4], blue: ['#7aa6ee', 3] };
    for (let i = 1; i < path.length; i++) {
      const a = path[i - 1];
      const b = path[i];
      const k = kind(Math.max(a.altitude, b.altitude));
      if (!k) continue;
      const pa = project(a.bearing, a.altitude);
      const pb = project(b.bearing, b.altitude);
      if (!pa || !pb || Math.abs(pa.x - pb.x) > width) continue;
      [g.strokeStyle, g.lineWidth] = STYLE[k];
      g.shadowBlur = 4;
      g.beginPath();
      g.moveTo(pa.x, pa.y);
      g.lineTo(pb.x, pb.y);
      g.stroke();
    }

    // The hours along it.
    g.fillStyle = '#fff';
    g.textAlign = 'center';
    for (const p of path) {
      if (p.m % 60 || p.altitude < -6) continue;
      const q = project(p.bearing, p.altitude);
      if (!q) continue;
      g.beginPath();
      g.arc(q.x, q.y, 3, 0, 2 * Math.PI);
      g.fill();
      g.fillText(clock(p.m), q.x, q.y - 9);
    }

    // Where it rises and sets, on the horizon.
    for (const [time, label] of [[light?.sunrise, t('calc.sun.sunrise')], [light?.sunset, t('calc.sun.sunset')]]) {
      if (!time) continue;
      const s = sunPosition(time, lat, lon);
      const q = project(s.bearing, 0);
      if (!q) continue;
      g.fillStyle = '#f2a33a';
      g.beginPath();
      g.moveTo(q.x, q.y + 3);
      g.lineTo(q.x - 6, q.y + 13);
      g.lineTo(q.x + 6, q.y + 13);
      g.fill();
      g.fillStyle = '#fff';
      g.fillText(`${label} ${clock(minuteOf(time))}`, q.x, q.y + 28);
    }

    // The sun at the time picked; below the horizon, its outline. Out of
    // view, an arrow to the side to turn to.
    const q = project(sun.bearing, sun.altitude);
    g.shadowBlur = 0;
    if (q && q.x > -20 && q.x < width + 20 && q.y > -20 && q.y < height + 20) {
      const r = Math.max(9, (0.27 * RAD * height) / 2 / Math.tan((view.fov * RAD) / 2));
      const halo = g.createRadialGradient(q.x, q.y, r * 0.5, q.x, q.y, r * 4);
      halo.addColorStop(0, 'rgba(255,236,190,.7)');
      halo.addColorStop(1, 'rgba(255,236,190,0)');
      g.fillStyle = halo;
      g.beginPath();
      g.arc(q.x, q.y, r * 4, 0, 2 * Math.PI);
      g.fill();
      g.beginPath();
      g.arc(q.x, q.y, r, 0, 2 * Math.PI);
      if (sun.altitude > -0.833) {
        g.fillStyle = '#fff4d6';
        g.fill();
      } else {
        g.strokeStyle = '#fff4d6';
        g.lineWidth = 2;
        g.stroke();
      }
    } else {
      const turn = ((sun.bearing - view.bearing + 540) % 360) - 180;
      const x = turn > 0 ? width - 18 : 18;
      const y = height / 2;
      const d = turn > 0 ? 1 : -1;
      g.fillStyle = '#fff4d6';
      g.shadowBlur = 4;
      g.beginPath();
      g.moveTo(x + d * 8, y);
      g.lineTo(x - d * 4, y - 9);
      g.lineTo(x - d * 4, y + 9);
      g.fill();
      g.textAlign = turn > 0 ? 'right' : 'left';
      g.fillText(t('calc.sun.view.sun'), x - d * 10, y + 4);
    }
  });

  /** Turns a panorama to face `bearing`, the horizon a little under the middle. */
  async function face(bearing) {
    if (!viewer || !spherical) return;
    const [pov, centre] = await Promise.all([viewer.getPointOfView(), viewer.getCenter()]);
    const turn = ((bearing - pov.bearing + 540) % 360) - 180;
    const lift = 6 - pov.tilt;
    viewer.setCenter([(((centre[0] + turn / 360) % 1) + 1) % 1, Math.min(1, Math.max(0, centre[1] - lift / 180))]);
  }
  function goTo(time) {
    if (!time) return;
    minutes = minuteOf(time);
    face(sunPosition(time, lat, lon).bearing);
  }
</script>

<div class="street" bind:clientWidth={width} bind:clientHeight={height}>
  <div class="viewer" bind:this={element}></div>
  {#if status === 'ready'}
    <div class="tone" style:background-image={tint} style:opacity={tone.tint}></div>
    <div class="glow" style:background-image={glow}></div>
    <div class="night" style:opacity={tone.dark}></div>
    <canvas bind:this={canvas}></canvas>
  {:else}
    <p class="status">{t(`calc.sun.view.${status}`)}</p>
  {/if}
</div>

{#if status === 'ready'}
  <div class="time">
    <label>
      <span>{t('calc.sun.view.time')} <strong>{clock(minutes)}</strong></span>
      <input type="range" min="0" max="1439" step="1" bind:value={minutes} style:background-image={track} />
    </label>
    <p class="where">{t(sun.altitude >= 0 ? 'calc.sun.view.up' : 'calc.sun.view.down', {
      altitude: n(Math.abs(sun.altitude), 0), bearing: n(sun.bearing, 0), side: compass(sun.bearing) })}</p>
    <div class="go">
      <button type="button" class="ghost" disabled={!light?.sunrise} onclick={() => goTo(light.sunrise)}>{t('calc.sun.view.sunrise')}</button>
      <button type="button" class="ghost" disabled={!light?.sunset} onclick={() => goTo(light.sunset)}>{t('calc.sun.view.sunset')}</button>
    </div>
  </div>
  <p class="note">{t('calc.sun.view.note', { distance: distance < 1000 ? `${n(distance, 0)} m` : `${n(distance / 1000, 1)} km` })}</p>
{/if}

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .street {
    /* The viewer's own layers kept inside, under the sticky header. */
    position: relative; isolation: isolate; margin-top: 16px;
    height: 380px; border-radius: 10px; overflow: hidden; background: var(--color-panel-2);
    @include phone { height: 300px; }
  }
  .viewer { position: absolute; inset: 0; }
  .tone, .glow, .night, canvas { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; }
  .tone { mix-blend-mode: soft-light; }
  .glow { mix-blend-mode: screen; opacity: .55; }
  .night { background: #0a1230; mix-blend-mode: multiply; }
  .status {
    position: absolute; inset: 0; display: grid; place-items: center; margin: 0; padding: 24px;
    text-align: center; font-size: 13.5px; line-height: 1.5; color: var(--color-muted);
  }

  .time {
    display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 8px 16px; align-items: end; margin-top: 14px;
    @include phone { grid-template-columns: 1fr; }
    label { display: grid; gap: 8px; }
    label > span { font-size: 12.5px; color: var(--color-muted); }
    strong { color: var(--color-ink); font-variant-numeric: tabular-nums; }
    .where { grid-column: 1 / -1; grid-row: 2; margin: 0; font-size: 13px; color: var(--color-muted); }
    .go { display: flex; gap: 8px; button { min-height: 36px; } }
  }
  /* The slider's track is the day's light, from night to night. */
  input[type='range'] {
    appearance: none; width: 100%; height: 12px; padding: 0; border: 0; border-radius: 999px;
    box-shadow: inset 0 0 0 1px var(--color-line);
    &::-webkit-slider-thumb {
      appearance: none; width: 18px; height: 18px; border-radius: 50%;
      background: #fff4d6; box-shadow: 0 0 0 2px #e8a33d;
    }
    &::-moz-range-thumb {
      width: 18px; height: 18px; border: 0; border-radius: 50%;
      background: #fff4d6; box-shadow: 0 0 0 2px #e8a33d;
    }
  }
  .note { margin: 12px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--color-muted); }
</style>
