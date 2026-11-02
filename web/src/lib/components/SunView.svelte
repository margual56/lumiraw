<script>
  /** The spot picked on the golden hour's map, as seen standing there: the
   *  land against the sky all the way round (lib/horizon.js), the sky in the
   *  colours of the time picked, and the sun's path for the day over it, so
   *  it shows whether the sunrise or the sunset can be seen from there or
   *  goes behind a hill first. Dragged sideways to look round. */
  import { t, n, i18n } from '$lib/i18n.svelte.js';
  import { sunPosition } from '$lib/calc.js';
  import { horizonFrom, landAt } from '$lib/horizon.js';

  /** `facing` and `fov`: which way the view looks, and how wide it is, in
   *  degrees; bound, so the map can show them. */
  let { lat, lon, day, light, facing = $bindable(270), fov = $bindable(100) } = $props();

  const RAD = Math.PI / 180;

  let status = $state('looking');
  let land = $state.raw(null);
  let width = $state(0);
  let height = $state(0);
  let minutes = $state(18 * 60);
  let canvas = $state(null);

  // -- the land -------------------------------------------------------------------

  let worked = 0;
  $effect(() => {
    const job = ++worked;
    status = 'looking';
    horizonFrom(lat, lon).then((found) => {
      if (job !== worked) return;
      land = found;
      status = 'ready';
    }, () => {
      if (job !== worked) return;
      // Level all round: the path still shows, the hills do not.
      land = null;
      status = 'failed';
    });
  });

  // -- the sun --------------------------------------------------------------------

  /** A minute of the day picked, as a Date in this device's time zone. */
  const at = (m) => {
    const date = new Date(`${day}T00:00`);
    date.setMinutes(m);
    return date;
  };
  const minuteOf = (date) => date.getHours() * 60 + date.getMinutes();

  /** The sun every minute of the day. */
  const path = $derived(Array.from({ length: 1441 }, (_, m) => ({ m, ...sunPosition(at(m), lat, lon) })));
  const sun = $derived(path[minutes]);
  const skyline = $derived(land?.layers.at(-1));
  // Behind the land once its top edge is, a quarter of a degree over its centre.
  const hidden = (p) => p.altitude + 0.27 < (skyline ? landAt(skyline, p.bearing) : 0);
  /** When the sun first clears the land in the morning and last goes behind it in the evening. */
  const seen = $derived.by(() => {
    const noon = light?.noon ? minuteOf(light.noon) : 720;
    let from = null;
    let to = null;
    for (let m = noon; m >= 0 && !hidden(path[m]); m--) from = m;
    for (let m = noon; m <= 1440 && !hidden(path[m]); m++) to = m;
    return { from: from > 0 ? from : null, to: to < 1440 ? to : null, never: hidden(path[noon]) };
  });

  // Each new day or place opens facing the sunset, on the evening's golden hour.
  $effect(() => {
    const start = light?.eveningGolden?.from ?? light?.sunset ?? light?.noon;
    if (start) minutes = minuteOf(start);
    const set = light?.sunset ?? light?.noon;
    if (set) facing = sunPosition(set, lat, lon).bearing;
  });

  /** The colour of the sky by the sun's altitude, at the top and at the horizon. */
  const SKY = [
    [-18, [4, 6, 14], [10, 14, 28]],
    [-12, [8, 14, 36], [22, 32, 64]],
    [-6, [20, 36, 86], [64, 84, 140]],
    [-4, [32, 56, 120], [124, 112, 160]],
    [-1, [52, 78, 150], [236, 130, 90]],
    [2, [70, 110, 180], [250, 160, 90]],
    [6, [80, 130, 200], [250, 200, 140]],
    [15, [70, 130, 210], [200, 220, 240]],
    [30, [60, 120, 210], [185, 212, 242]],
  ];
  function sky(altitude) {
    const i = SKY.findIndex(([h]) => h > altitude);
    if (i <= 0) {
      const [, top, horizon] = SKY[i === 0 ? 0 : SKY.length - 1];
      return { top, horizon };
    }
    const [h0, top0, horizon0] = SKY[i - 1];
    const [h1, top1, horizon1] = SKY[i];
    const k = (altitude - h0) / (h1 - h0);
    const mix = (a, b) => a.map((x, j) => Math.round(x + (b[j] - x) * k));
    return { top: mix(top0, top1), horizon: mix(horizon0, horizon1) };
  }
  const rgb = (c, alpha = 1) => `rgba(${c.join(',')},${alpha})`;
  /** Two colours mixed, `k` of the way to the second. */
  const blend = (a, b, k) => a.map((x, j) => Math.round(x + (b[j] - x) * k));

  /** The day's light along the slider: night, blue, gold, day, and back. */
  const track = $derived(`linear-gradient(to right, ${path.filter((p) => p.m % 20 === 0).map((p) => {
    const s = sky(p.altitude);
    return `${rgb(blend(s.top, s.horizon, 0.5))} ${((p.m / 1440) * 100).toFixed(1)}%`;
  }).join(', ')})`);

  const clock = (m) => new Intl.DateTimeFormat(i18n.locale, { hour: '2-digit', minute: '2-digit' }).format(at(m));
  const POINTS = $derived(t('calc.sun.compass').split(','));
  const compass = (bearing) => POINTS[Math.round(bearing / 22.5) % 16];

  // -- the drawing ----------------------------------------------------------------

  /** Pixels a degree, and where level ground meets the sky. */
  const scale = $derived(width / fov);
  const level = $derived(height * 0.66);
  const turnFrom = (bearing) => ((bearing - facing + 540) % 360) - 180;
  const xOf = (bearing) => width / 2 + turnFrom(bearing) * scale;
  const yOf = (altitude) => level - altitude * scale;

  $effect(() => {
    if (!canvas || !width || !height) return;
    const ratio = devicePixelRatio || 1;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    const g = canvas.getContext('2d');
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    const tone = sky(sun.altitude);
    const left = facing - fov / 2;

    // The sky, and a glow about the sun while it is low.
    const air = g.createLinearGradient(0, 0, 0, level);
    air.addColorStop(0, rgb(tone.top));
    air.addColorStop(1, rgb(tone.horizon));
    g.fillStyle = air;
    g.fillRect(0, 0, width, height);
    const sx = xOf(sun.bearing);
    const sy = yOf(sun.altitude);
    if (sun.altitude > -8 && sun.altitude < 15) {
      const warmth = 1 - Math.abs(sun.altitude - 1) / 14;
      const glow = g.createRadialGradient(sx, sy, 0, sx, sy, Math.max(width, height) * 0.5);
      glow.addColorStop(0, `rgba(255,190,110,${0.75 * warmth})`);
      glow.addColorStop(1, 'rgba(255,190,110,0)');
      g.fillStyle = glow;
      g.fillRect(0, 0, width, height);
    }

    // The sun at the time picked, before the land, which hides it where it stands higher.
    if (sx > -30 && sx < width + 30) {
      const r = Math.max(7, 0.27 * scale);
      const halo = g.createRadialGradient(sx, sy, r * 0.5, sx, sy, r * 4);
      halo.addColorStop(0, 'rgba(255,236,190,.65)');
      halo.addColorStop(1, 'rgba(255,236,190,0)');
      g.fillStyle = halo;
      g.beginPath();
      g.arc(sx, sy, r * 4, 0, 2 * Math.PI);
      g.fill();
      g.fillStyle = '#fff4d6';
      g.beginPath();
      g.arc(sx, sy, r, 0, 2 * Math.PI);
      g.fill();
    } else {
      const d = turnFrom(sun.bearing) > 0 ? 1 : -1;
      const x = d > 0 ? width - 18 : 18;
      const y = Math.max(20, Math.min(level - 30, sy));
      g.fillStyle = '#fff4d6';
      g.beginPath();
      g.moveTo(x + d * 8, y);
      g.lineTo(x - d * 4, y - 9);
      g.lineTo(x - d * 4, y + 9);
      g.fill();
      g.textAlign = d > 0 ? 'right' : 'left';
      g.fillText(t('calc.sun.view.sun'), x - d * 10, y + 4);
    }

    // The land, far to near: the far hills paler with the air between.
    const lit = Math.max(0, Math.min(1, (sun.altitude + 8) / 20));
    const ground = blend([6, 8, 14], [34, 38, 40], lit);
    const layers = land?.layers ?? [new Float32Array(720)];
    layers.slice().reverse().forEach((line, i, all) => {
      const haze = all.length > 1 ? (all.length - 1 - i) / (all.length - 1) : 0;
      g.fillStyle = rgb(blend(ground, tone.horizon, 0.5 * haze));
      g.beginPath();
      g.moveTo(0, height);
      for (let x = 0; x < width + 2; x += 2) {
        const at = Math.min(x, width);
        g.lineTo(at, Math.min(height, yOf(Math.max(-30, landAt(line, left + at / scale)))));
      }
      g.lineTo(width, height);
      g.fill();
    });

    g.lineCap = 'round';
    g.font = '600 12px system-ui, sans-serif';
    g.textAlign = 'center';

    // Level, as the horizon would be with nothing on it.
    g.setLineDash([6, 6]);
    g.strokeStyle = 'rgba(255,255,255,.45)';
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(0, level);
    g.lineTo(width, level);
    g.stroke();
    g.setLineDash([]);

    // The compass along the bottom.
    g.fillStyle = 'rgba(255,255,255,.7)';
    for (let b = Math.ceil(left / 15) * 15; b <= left + fov; b += 15) {
      const x = xOf(b);
      const major = b % 45 === 0;
      g.fillRect(x - 0.5, height - (major ? 26 : 20), 1, major ? 8 : 4);
      if (major) g.fillText(t('calc.sun.points').split(',')[((b / 45) % 8 + 8) % 8], x, height - 6);
    }

    // The path, in stretches coloured by the light; fainter behind the land.
    const STYLE = { day: ['255,244,214', 2], golden: ['242,163,58', 4], blue: ['122,166,238', 3] };
    const kind = (altitude) => (altitude > 6 ? 'day' : altitude > -0.833 ? 'golden' : altitude > -6 ? 'blue' : null);
    for (let i = 1; i < path.length; i++) {
      const a = path[i - 1];
      const b = path[i];
      const k = kind(Math.max(a.altitude, b.altitude));
      if (!k) continue;
      const xa = xOf(a.bearing);
      const xb = xOf(b.bearing);
      if (Math.abs(xa - xb) > width / 2) continue;
      const [colour, thick] = STYLE[k];
      g.strokeStyle = `rgba(${colour},${hidden(b) ? 0.3 : 0.95})`;
      g.lineWidth = thick;
      g.beginPath();
      g.moveTo(xa, yOf(a.altitude));
      g.lineTo(xb, yOf(b.altitude));
      g.stroke();
    }

    // The hours along it.
    g.fillStyle = '#fff';
    g.shadowColor = 'rgba(0,0,0,.6)';
    g.shadowBlur = 4;
    for (const p of path) {
      if (p.m % 60 || p.altitude < -6 || p.m === 1440) continue;
      // Not over the marks of where it clears the land and goes behind it.
      if ([seen.from, seen.to].some((m) => m != null && Math.abs(m - p.m) < 20)) continue;
      const x = xOf(p.bearing);
      const y = yOf(p.altitude);
      g.beginPath();
      g.arc(x, y, 2.5, 0, 2 * Math.PI);
      g.fill();
      g.fillText(clock(p.m), x, y - 9);
    }

    // Where it clears the land and goes behind it.
    for (const m of [seen.from, seen.to]) {
      if (m == null) continue;
      const p = path[m];
      const x = xOf(p.bearing);
      const y = yOf(p.altitude);
      g.strokeStyle = '#fff4d6';
      g.lineWidth = 2;
      g.beginPath();
      g.arc(x, y, 6, 0, 2 * Math.PI);
      g.stroke();
      g.fillText(clock(m), x, y + 22);
    }

    // Behind the land, its outline, to keep it in sight.
    if (hidden(sun) && sx > -30 && sx < width + 30) {
      g.strokeStyle = '#fff4d6';
      g.lineWidth = 2;
      g.beginPath();
      g.arc(sx, sy, Math.max(7, 0.27 * scale), 0, 2 * Math.PI);
      g.stroke();
    }
  });

  // -- looking round --------------------------------------------------------------

  let dragged = null;
  function grab(event) {
    dragged = { x: event.clientX, facing };
    event.currentTarget.setPointerCapture(event.pointerId);
  }
  function drag(event) {
    if (!dragged) return;
    facing = (((dragged.facing - (event.clientX - dragged.x) / scale) % 360) + 360) % 360;
  }
  function keys(event) {
    const by = { ArrowLeft: -5, ArrowRight: 5 }[event.key];
    if (!by) return;
    event.preventDefault();
    facing = (facing + by + 360) % 360;
  }
  function goTo(time) {
    if (!time) return;
    minutes = minuteOf(time);
    facing = sunPosition(time, lat, lon).bearing;
  }
  const street = $derived(`https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=${lat},${lon}&heading=${Math.round(facing)}`);
</script>

<div class="view" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas bind:this={canvas} tabindex="0" aria-label={t('calc.sun.view.label')}
          onpointerdown={grab} onpointermove={drag} onpointerup={() => (dragged = null)}
          onpointercancel={() => (dragged = null)} onkeydown={keys}></canvas>
  {#if status === 'looking'}<p class="status">{t('calc.sun.view.looking')}</p>{/if}
</div>

<div class="time">
  <label>
    <span>{t('calc.sun.view.time')} <strong>{clock(minutes)}</strong></span>
    <input type="range" min="0" max="1439" step="1" bind:value={minutes} style:background-image={track} />
  </label>
  <div class="go">
    <button type="button" class="ghost" disabled={!light?.sunrise} onclick={() => goTo(light.sunrise)}>{t('calc.sun.view.sunrise')}</button>
    <button type="button" class="ghost" disabled={!light?.sunset} onclick={() => goTo(light.sunset)}>{t('calc.sun.view.sunset')}</button>
    <button type="button" class="ghost" aria-pressed={fov === 360} onclick={() => (fov = fov === 360 ? 100 : 360)}>{t('calc.sun.view.around')}</button>
  </div>
</div>
<p class="sunline">
  {t(sun.altitude >= 0 ? 'calc.sun.view.up' : 'calc.sun.view.down', {
    altitude: n(Math.abs(sun.altitude), Math.abs(sun.altitude) < 10 ? 1 : 0), bearing: n(sun.bearing, 0), side: compass(sun.bearing) })}
  {#if status === 'ready'}
    {#if seen.never}{t('calc.sun.view.never')}
    {:else}{t('calc.sun.view.seen', { from: seen.from == null ? '—' : clock(seen.from), to: seen.to == null ? '—' : clock(seen.to) })}{/if}
  {/if}
</p>
<p class="note">
  {status === 'failed' ? t('calc.sun.view.failed') : t('calc.sun.view.note', { ground: n(land?.ground ?? 0, 0) })}
  <a href={street} target="_blank" rel="noopener">{t('calc.sun.view.street')}</a>
</p>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .view {
    position: relative; margin-top: 16px; height: 340px; border-radius: 10px; overflow: hidden;
    background: var(--color-panel-2);
    @include phone { height: 260px; }
  }
  canvas {
    display: block; width: 100%; height: 100%; cursor: grab;
    /* Sideways is looking round; up and down still scroll the page. */
    touch-action: pan-y;
    &:active { cursor: grabbing; }
    &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -2px; }
  }
  .status {
    position: absolute; inset: auto 0 0 0; margin: 0; padding: 10px 14px; text-align: center;
    font-size: 13px; color: #fff; background: rgba(0, 0, 0, .45);
  }

  .time {
    display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px 16px; align-items: end; margin-top: 14px;
    @include phone { grid-template-columns: 1fr; }
    label { display: grid; gap: 8px; }
    label > span { font-size: 12.5px; color: var(--color-muted); }
    strong { color: var(--color-ink); font-variant-numeric: tabular-nums; }
    .go { display: flex; flex-wrap: wrap; gap: 8px; button { min-height: 36px; } }
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
  .sunline { margin: 10px 0 0; font-size: 13.5px; line-height: 1.5; }
  .note { margin: 10px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--color-muted); }
</style>
