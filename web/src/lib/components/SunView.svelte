<script>
  /** The spot picked on the golden hour's map, as seen standing there: the
   *  land against the sky all the way round (lib/horizon.js), the sky in the
   *  colours of the time picked, and the sun's path for the day over it, so
   *  it shows whether the sunrise or the sunset can be seen from there or
   *  goes behind a hill first. Dragged sideways to look round. By night,
   *  the stars, the planets, the moon and the Milky Way, as many as the sky
   *  there lets through; and the clouds forecast for the next day. */
  import { i18n, t, n, clock } from '$lib/i18n.svelte.js';
  import { lightOf, skyFrom, sunCoords, sunPosition } from '$lib/calc.js';
  import { compassPoint, wrap360 } from '$lib/geo.js';
  import { STEP, horizonFrom, landAt } from '$lib/horizon.js';
  import { CORE, faintest, moonFrom, moonPhase, nightSky, skyBrightness } from '$lib/night.js';
  import { drawSky } from '$lib/drawsky.js';
  import { skyAt } from '$lib/darksky.js';
  import { clouds, hourOf } from '$lib/weather.js';
  import { GROW, SHRINK } from '$lib/map.js';
  import '$lib/map.css';
  import SkyDome from './SkyDome.svelte';
  import CloudStrip from './CloudStrip.svelte';

  /** `facing` and `shown`: which way the view looks, and how wide it is, in
   *  degrees; bound, so the map can show them. */
  let { lat, lon, day, light, facing = $bindable(270), shown = $bindable(100) } = $props();
  /** How wide a view was asked for: the usual, or all round. */
  let fov = $state(100);

  /** The sun's half-width in degrees, its colour, and how far it may be out of view and still drawn. */
  const SUN_RADIUS = 0.27;
  const SUN = '#fff4d6';
  const MARGIN = 30;

  let status = $state('looking');
  let land = $state.raw(null);
  let width = $state(0);
  let height = $state(0);
  let minutes = $state(18 * 60);
  let canvas = $state(null);
  /** The panorama, or the whole sky overhead. */
  let dome = $state(false);
  /** Taller, as the maps can be. */
  let tall = $state(false);

  /**
   * `load` the spot's data a moment after it stops changing, so typing a
   * coordinate asks once, not once a keystroke; `done` or `failed` hear only
   * the latest. Gives the effect's clean-up.
   */
  function forSpot(load, done, failed = () => {}) {
    const spot = [lat, lon];
    let stale = false;
    const wait = setTimeout(() => load(...spot).then((x) => stale || done(x), () => stale || failed()), 300);
    return () => {
      stale = true;
      clearTimeout(wait);
    };
  }

  // -- the land -------------------------------------------------------------------

  $effect(() => {
    status = 'looking';
    return forSpot(horizonFrom, (found) => {
      land = found;
      status = 'ready';
    }, () => {
      // Level all round: the path still shows, the hills do not.
      land = null;
      status = 'failed';
    });
  });

  // -- the sun --------------------------------------------------------------------

  /** A minute of the day picked, as a Date in this device's time zone. */
  const midnight = $derived(new Date(`${day}T00:00`).valueOf());
  const at = (m) => {
    const date = new Date(midnight);
    date.setMinutes(m);
    return date;
  };
  const hm = (m) => clock(at(m));
  const minuteOf = (date) => date.getHours() * 60 + date.getMinutes();

  /** The sun every minute of the day. */
  const path = $derived(Array.from({ length: 1441 }, (_, m) => ({ m, ...sunPosition(at(m), lat, lon) })));
  /** The slider runs on to noon the next day, through the night. */
  const END = 2160;
  const sun = $derived(path[minutes] ?? { m: minutes, ...sunPosition(at(minutes), lat, lon) });
  const skyline = $derived(land?.layers.at(-1));
  /** Whether the land hides the sun, minute by minute: once its top edge is behind it. */
  const hidden = $derived(path.map((p) => p.altitude + SUN_RADIUS < (skyline ? landAt(skyline, p.bearing) : 0)));
  /** When the sun first clears the land in the morning and last goes behind it in the evening. */
  const seen = $derived.by(() => {
    const noon = light?.noon ? minuteOf(light.noon) : 720;
    let from = null;
    let to = null;
    for (let m = noon; m >= 0 && !hidden[m]; m--) from = m;
    for (let m = noon; m <= 1440 && !hidden[m]; m++) to = m;
    return { from: from > 0 ? from : null, to: to < 1440 ? to : null, never: hidden[noon] };
  });

  // Each new day or place opens facing the sunset, on the evening's golden hour.
  $effect(() => {
    const start = light?.eveningGolden?.from ?? light?.sunset ?? light?.noon;
    if (start) minutes = minuteOf(start);
    const set = light?.sunset ?? light?.noon;
    if (set) facing = path[minuteOf(set)].bearing;
  });

  // -- the night ------------------------------------------------------------------

  /** How far into the night the time picked is: 0 by day, 1 once the stars are all out. */
  const dark = $derived(Math.max(0, Math.min(1, (-sun.geometric - 4) / 10)));

  /** The stars, and the light on the ground under the sky (the atlas's
   *  brightness ratio), each fetched the first time the night needs them. */
  let data = $state.raw(null);
  let pollution = $state.raw(null);
  $effect(() => {
    if (nightly && !data) import('$lib/sky-data.js').then((loaded) => (data = loaded));
  });
  const nightly = $derived(dark > 0 || dome);
  $effect(() => {
    if (!nightly) return;
    pollution = null;
    // Outside the atlas, as if unlit.
    return forSpot(skyAt, (found) => (pollution = found ?? { ratio: 0, zone: null }));
  });

  /** The clouds over the next day. */
  let forecast = $state.raw(null);
  $effect(() => {
    forecast = null;
    return forSpot(clouds, (hours) => (forecast = hours), () => (forecast = 'failed'));
  });
  const cloudNow = $derived(Array.isArray(forecast) ? hourOf(forecast, at(minutes)) : null);

  const sky = $derived(data && nightly ? nightSky(data, at(minutes), lat, lon, i18n.locale) : null);
  /** How bright the sky is at the time picked, the faintest star it lets through, and how plain the Milky Way is. */
  const glow = $derived(skyBrightness({
    ratio: pollution?.ratio ?? 0, sun: sun.geometric, moon: sky?.moon.altitude ?? -90, lit: sky?.moon.lit ?? 0,
  }));
  const limit = $derived(faintest(glow));
  const night = $derived({
    limit, dark, milky: Math.max(0, Math.min(1, (glow - 19.8) / 1.4)),
    planetName: (name) => t(`calc.sun.night.planet.${name}`),
    milkyName: t('calc.sun.night.milkyWay'),
  });

  /** The night after the day picked, noon to noon, every five minutes:
   *  how far down the sun is, how high the moon and the galaxy's core. */
  const overnight = $derived.by(() => {
    const out = [];
    for (let m = 720; m <= END; m += 5) {
      const date = at(m);
      const look = skyFrom(date, lat, lon);
      const s = sunCoords(date);
      out.push({
        m, sun: look(s.ra, s.dec).geometric,
        // Its top edge.
        moon: moonFrom(look, date).altitude + 0.27,
        core: look(CORE.ra, CORE.dec),
      });
    }
    return out;
  });

  /** When the sky is fully dark that night, the sun 18° down. */
  const fullDark = $derived.by(() => {
    const black = overnight.filter((p) => p.sun < -18);
    return black.length ? { from: black[0].m, until: black.at(-1).m } : null;
  });

  /** The moon that night: when it rises and sets, and how much of it is lit. */
  const moonNight = $derived.by(() => {
    let rise = null;
    let set = null;
    for (let i = 1; i < overnight.length; i++) {
      const was = overnight[i - 1].moon > 0;
      const is = overnight[i].moon > 0;
      if (is && !was) rise ??= overnight[i].m;
      if (was && !is) set ??= overnight[i].m;
    }
    const middle = at(fullDark ? (fullDark.from + fullDark.until) / 2 : 1440);
    return { rise, set, up: overnight[0].moon > 0, ...moonPhase(middle) };
  });

  /** The galaxy's core while it is dark: when it stands highest above the land, and how high. */
  const core = $derived.by(() => {
    let best = null;
    for (const { m, sun: down, core: p } of overnight) {
      if (down >= -18 || p.altitude <= (skyline ? landAt(skyline, p.bearing) : 0)) continue;
      if (!best || p.altitude > best.altitude) best = { m, ...p };
    }
    return best;
  });

  /** What is up at the time picked and clear of the land: the planets the sky lets through, and the best-known constellations. */
  const above = (p, by = 0) => p.altitude > (skyline ? landAt(skyline, p.bearing) : 0) + by;
  const upNow = $derived(!sky || dark < 0.5 ? null : {
    planets: sky.planets.filter((p) => p.mag < limit && above(p, 1)).map((p) => night.planetName(p.name)),
    constellations: sky.labels.filter((c) => c.rank === 1 && above(c, 10))
      .sort((a, b) => b.altitude - a.altitude).slice(0, 8).map((c) => c.name),
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
  function toneOf(altitude) {
    const i = SKY.findIndex(([h]) => h > altitude);
    if (i <= 0) {
      const [, top, horizon] = SKY[i === 0 ? 0 : SKY.length - 1];
      return { top, horizon };
    }
    const [h0, top0, horizon0] = SKY[i - 1];
    const [h1, top1, horizon1] = SKY[i];
    const k = (altitude - h0) / (h1 - h0);
    return { top: blend(top0, top1, k), horizon: blend(horizon0, horizon1, k) };
  }
  const rgb = (c, alpha = 1) => `rgba(${c.join(',')},${alpha})`;
  /** Two colours mixed, `k` of the way to the second. */
  const blend = (a, b, k) => a.map((x, j) => Math.round(x + (b[j] - x) * k));

  /** The day's light along the slider: night, blue, gold, day, and back. */
  const track = $derived(`linear-gradient(to right, ${Array.from({ length: END / 20 + 1 }, (_, i) => {
    const m = i * 20;
    const s = toneOf((path[m] ?? sunPosition(at(m), lat, lon)).altitude);
    return `${rgb(blend(s.top, s.horizon, 0.5))} ${((m / END) * 100).toFixed(1)}%`;
  }).join(', ')})`);

  /** The sky's colours at the time picked, and the land's. */
  const tone = $derived(toneOf(sun.altitude));
  const ground = $derived(blend([6, 8, 14], [34, 38, 40], Math.max(0, Math.min(1, (sun.altitude + 8) / 20))));

  const POINTS = $derived(t('calc.sun.compass').split(','));
  const SHORT = $derived(t('calc.sun.points').split(','));

  // -- the drawing ----------------------------------------------------------------

  /** Where level ground meets the sky: a third of the way up, and less ground when the view is tall. */
  const level = $derived(Math.max(height * 0.66, height - 140));
  /** How wide the view is: as asked, but when tall, no wider than keeps its top at the zenith. */
  const span = $derived(tall && fov < 360 && width && level ? Math.min(fov, (width * 90) / level) : fov);
  $effect(() => {
    shown = span;
  });
  /** Pixels a degree. */
  const scale = $derived(width / span);
  const turnFrom = (bearing) => wrap360(bearing - facing + 180) - 180;
  const xOf = (bearing) => width / 2 + turnFrom(bearing) * scale;
  const yOf = (altitude) => level - altitude * scale;

  $effect(() => {
    if (!canvas || !width || !height || dome) return;
    const ratio = devicePixelRatio || 1;
    // Resized only when it changes: setting the size clears and reallocates it.
    if (canvas.width !== Math.round(width * ratio)) canvas.width = Math.round(width * ratio);
    if (canvas.height !== Math.round(height * ratio)) canvas.height = Math.round(height * ratio);
    const g = canvas.getContext('2d');
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.shadowBlur = 0;
    const left = facing - span / 2;

    // The sky, and a glow about the sun while it is low.
    const air = g.createLinearGradient(0, 0, 0, level);
    air.addColorStop(0, rgb(tone.top));
    air.addColorStop(1, rgb(tone.horizon));
    g.fillStyle = air;
    g.fillRect(0, 0, width, height);
    const sx = xOf(sun.bearing);
    const sy = yOf(sun.altitude);
    const r = Math.max(7, SUN_RADIUS * scale);
    const inView = sx > -MARGIN && sx < width + MARGIN;
    if (sun.altitude > -8 && sun.altitude < 15) {
      const warmth = 1 - Math.abs(sun.altitude - 1) / 14;
      const glow = g.createRadialGradient(sx, sy, 0, sx, sy, Math.max(width, height) * 0.5);
      glow.addColorStop(0, `rgba(255,190,110,${0.75 * warmth})`);
      glow.addColorStop(1, 'rgba(255,190,110,0)');
      g.fillStyle = glow;
      g.fillRect(0, 0, width, height);
    }

    // The night sky, before the land, which hides what is behind it.
    if (sky) {
      drawSky(g, sky, {
        ...night, width, height, scale, sun,
        place: (bearing, altitude) => {
          const x = xOf(bearing);
          return x < -20 || x > width + 20 ? null : [x, yOf(altitude)];
        },
        broken: (a, b) => Math.abs(a[0] - b[0]) > width / 2,
        ranks: span > 180 ? 1 : 2,
      });
    }

    // The sun at the time picked, before the land, which hides it where it stands higher.
    if (inView) {
      const halo = g.createRadialGradient(sx, sy, r * 0.5, sx, sy, r * 4);
      halo.addColorStop(0, 'rgba(255,236,190,.65)');
      halo.addColorStop(1, 'rgba(255,236,190,0)');
      g.fillStyle = halo;
      g.beginPath();
      g.arc(sx, sy, r * 4, 0, 2 * Math.PI);
      g.fill();
      g.fillStyle = SUN;
      g.beginPath();
      g.arc(sx, sy, r, 0, 2 * Math.PI);
      g.fill();
    } else {
      const d = turnFrom(sun.bearing) > 0 ? 1 : -1;
      const x = d > 0 ? width - 18 : 18;
      const y = Math.max(20, Math.min(level - 30, sy));
      g.fillStyle = SUN;
      g.beginPath();
      g.moveTo(x + d * 8, y);
      g.lineTo(x - d * 4, y - 9);
      g.lineTo(x - d * 4, y + 9);
      g.fill();
      g.textAlign = d > 0 ? 'right' : 'left';
      g.fillText(t('calc.sun.view.sun'), x - d * 10, y + 4);
    }

    // The land, far to near: the far hills paler with the air between.
    const layers = land?.layers ?? [new Float32Array(360 / STEP)];
    for (let i = layers.length - 1; i >= 0; i--) {
      const line = layers[i];
      const haze = layers.length > 1 ? i / (layers.length - 1) : 0;
      g.fillStyle = rgb(blend(ground, tone.horizon, 0.5 * haze));
      g.beginPath();
      g.moveTo(0, height);
      for (let x = 0; x < width + 2; x += 2) {
        const at = Math.min(x, width);
        g.lineTo(at, Math.min(height, yOf(Math.max(-30, landAt(line, left + at / scale)))));
      }
      g.lineTo(width, height);
      g.fill();
    }

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
    for (let b = Math.ceil(left / 15) * 15; b <= left + span; b += 15) {
      const x = xOf(b);
      const major = b % 45 === 0;
      g.fillRect(x - 0.5, height - (major ? 26 : 20), 1, major ? 8 : 4);
      if (major) g.fillText(compassPoint(b, SHORT), x, height - 6);
    }

    // The path, in stretches coloured by the light as the table under the
    // map names it; fainter behind the land. Each stretch is one stroke.
    const STYLE = {
      day: ['255,244,214', 2], golden: ['242,163,58', 4], dusk: ['176,170,214', 2], blue: ['122,166,238', 3],
    };
    let stretch = null;
    const finish = () => {
      if (stretch) g.stroke();
      stretch = null;
    };
    for (let i = 1; i < path.length; i++) {
      const a = path[i - 1];
      const b = path[i];
      const k = lightOf(Math.max(a.geometric, b.geometric));
      const xa = xOf(a.bearing);
      const xb = xOf(b.bearing);
      // Night, out of view, or wrapping round behind the viewer.
      if (!k || (Math.max(xa, xb) < 0) || Math.min(xa, xb) > width || Math.abs(xa - xb) > width / 2) {
        finish();
        continue;
      }
      const style = `${k}${hidden[i] ? ':behind' : ''}`;
      if (style !== stretch) {
        finish();
        const [colour, thick] = STYLE[k];
        g.strokeStyle = `rgba(${colour},${hidden[i] ? 0.3 : 0.95})`;
        g.lineWidth = thick;
        g.beginPath();
        g.moveTo(xa, yOf(a.altitude));
        stretch = style;
      }
      g.lineTo(xb, yOf(b.altitude));
    }
    finish();

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
      g.fillText(hm(p.m), x, y - 9);
    }

    // Where it clears the land and goes behind it.
    for (const m of [seen.from, seen.to]) {
      if (m == null) continue;
      const p = path[m];
      const x = xOf(p.bearing);
      const y = yOf(p.altitude);
      g.strokeStyle = SUN;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(x, y, 6, 0, 2 * Math.PI);
      g.stroke();
      g.fillText(hm(m), x, y + 22);
    }

    // Behind the land, its outline, to keep it in sight.
    if (hidden[minutes] && inView && sun.altitude > -8) {
      g.strokeStyle = SUN;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(sx, sy, r, 0, 2 * Math.PI);
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
    facing = wrap360(dragged.facing - (event.clientX - dragged.x) / scale);
  }
  function keys(event) {
    const by = { ArrowLeft: -5, ArrowRight: 5 }[event.key];
    if (!by) return;
    event.preventDefault();
    facing = wrap360(facing + by);
  }
  function goTo(time) {
    if (!time) return;
    minutes = minuteOf(time);
    facing = path[minutes].bearing;
  }
  /** To the evening once it is fully dark, looking south (north south of the equator), where the Milky Way's core is. */
  function tonight() {
    minutes = Math.min(END - 1, fullDark.from);
    facing = core?.bearing ?? (lat >= 0 ? 180 : 0);
  }
  const street = $derived(`https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=${lat},${lon}&heading=${Math.round(facing)}`);
</script>

<div class="view" class:dome class:tall bind:clientWidth={width} bind:clientHeight={height}>
  {#if dome}
    <SkyDome {sky} {night} {tone} {ground} {skyline} {path} {sun} bind:facing fov={span} />
  {:else}
    <canvas bind:this={canvas} tabindex="0" aria-label={t('calc.sun.view.label')}
            onpointerdown={grab} onpointermove={drag} onpointerup={() => (dragged = null)}
            onpointercancel={() => (dragged = null)} onkeydown={keys}></canvas>
  {/if}
  {#if cloudNow}
    <p class="cloudy">{t('calc.sun.clouds.now', { total: cloudNow.total, low: cloudNow.low })}</p>
  {/if}
  <button type="button" class="map-grow grow" aria-label={t(tall ? 'calc.view.smaller' : 'calc.view.bigger')}
          title={t(tall ? 'calc.view.smaller' : 'calc.view.bigger')} onclick={() => (tall = !tall)}>
    {@html tall ? SHRINK : GROW}
  </button>
  {#if status === 'looking'}<p class="status">{t('calc.sun.view.looking')}</p>{/if}
</div>

<div class="time">
  <label>
    <span>{t('calc.sun.view.time')} <strong>{hm(minutes)}</strong>{minutes >= 1440 ? ` ${t('calc.sun.view.nextDay')}` : ''}</span>
    <!-- The track is the day's light, from night to night. -->
    <input type="range" min="0" max={END - 1} step="1" bind:value={minutes} style:background-image={track} />
  </label>
  <div class="go">
    <button type="button" class="ghost" disabled={!light?.sunrise} onclick={() => goTo(light.sunrise)}>{t('calc.sun.view.sunrise')}</button>
    <button type="button" class="ghost" disabled={!light?.sunset} onclick={() => goTo(light.sunset)}>{t('calc.sun.view.sunset')}</button>
    <button type="button" class="ghost" disabled={!fullDark} onclick={tonight}>{t('calc.sun.night.tonight')}</button>
    <button type="button" class="ghost" aria-pressed={fov === 360} onclick={() => (fov = fov === 360 ? 100 : 360)}>{t('calc.sun.view.around')}</button>
    <button type="button" class="ghost" aria-pressed={dome} onclick={() => (dome = !dome)}>{t('calc.sun.night.whole')}</button>
  </div>
</div>
<p class="sunline">
  {t(sun.altitude >= 0 ? 'calc.sun.view.up' : 'calc.sun.view.down', {
    altitude: n(Math.abs(sun.altitude), Math.abs(sun.altitude) < 10 ? 1 : 0), bearing: n(sun.bearing, 0), side: compassPoint(sun.bearing, POINTS) })}
  {#if status === 'ready'}
    {#if seen.never}{t('calc.sun.view.never')}
    {:else}{t('calc.sun.view.seen', { from: seen.from == null ? '—' : hm(seen.from), to: seen.to == null ? '—' : hm(seen.to) })}{/if}
  {/if}
</p>
<dl class="night">
  <div>
    <dt>{t('calc.sun.night.dark')}</dt>
    <dd>{fullDark ? t('calc.sun.night.darkSpan', { from: hm(fullDark.from), until: hm(fullDark.until) }) : t('calc.sun.night.never')}</dd>
  </div>
  <div>
    <dt>{t('calc.sun.night.moon')}</dt>
    <dd>{t(moonNight.waxing ? 'calc.sun.night.waxing' : 'calc.sun.night.waning', { lit: Math.round(moonNight.lit * 100) })}
      {moonNight.rise == null && moonNight.set == null ? t(moonNight.up ? 'calc.sun.night.moonAllNight' : 'calc.sun.night.moonNoNight')
        : [moonNight.rise != null && t('calc.sun.night.rises', { time: hm(moonNight.rise) }),
           moonNight.set != null && t('calc.sun.night.sets', { time: hm(moonNight.set) })].filter(Boolean).join(' ')}</dd>
  </div>
  <div>
    <dt>{t('calc.sun.night.core')}</dt>
    <dd>{core ? t('calc.sun.night.coreBest', { time: hm(core.m), altitude: n(core.altitude, 0), side: compassPoint(core.bearing, POINTS) })
      : t('calc.sun.night.coreNever')}</dd>
  </div>
  {#if dark > 0}
    <div>
      <dt>{t('calc.sun.night.faintest')}</dt>
      <dd>{t('calc.sun.night.limit', { mag: n(limit, 1) })}
        {pollution?.zone ? t('calc.sun.night.zone', { zone: pollution.zone }) : ''}
        {sky?.moon.altitude > 0 && sky.moon.lit > 0.25 ? t('calc.sun.night.moonlit') : ''}</dd>
    </div>
  {/if}
  {#if upNow}
    <div>
      <dt>{t('calc.sun.night.up')}</dt>
      <dd>{[...upNow.planets, ...upNow.constellations].join(', ') || '—'}</dd>
    </div>
  {/if}
</dl>
<CloudStrip {lat} {lon} hours={forecast} picked={cloudNow} />
<p class="note">
  {status === 'failed' ? t('calc.sun.view.failed') : t('calc.sun.view.note', { ground: n(land?.ground ?? 0, 0) })}
  <a href={street} target="_blank" rel="noopener">{t('calc.sun.view.street')}</a>
</p>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .view {
    position: relative; margin-top: 8px; height: 340px; border-radius: 10px; overflow: hidden;
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
  .view.dome { height: min(560px, 92vw); }
  .view.tall { height: min(82vh, 960px); }
  /* Drawn as the maps' is (map.css). */
  .grow { position: absolute; top: 8px; right: 8px; border-radius: 4px; box-shadow: 0 1px 5px rgba(0, 0, 0, .4); }
  .cloudy {
    position: absolute; top: 8px; left: 8px; margin: 0; padding: 3px 8px; border-radius: 6px;
    font-size: 12px; color: #fff; background: rgba(0, 0, 0, .45); pointer-events: none;
  }
  .night {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 10px 18px; margin: 14px 0 0;
    div { min-width: 0; }
    dt { font-size: 12px; color: var(--color-muted); }
    dd { margin: 2px 0 0; font-size: 13.5px; line-height: 1.45; }
  }
  .status {
    position: absolute; inset: auto 0 0 0; margin: 0; padding: 10px 14px; text-align: center;
    font-size: 13px; color: #fff; background: rgba(0, 0, 0, .45);
  }

  .time {
    display: grid; gap: 12px; margin-top: 14px;
    label { display: grid; gap: 8px; }
    label > span { font-size: 12.5px; color: var(--color-muted); }
    strong { color: var(--color-ink); font-variant-numeric: tabular-nums; }
    .go {
      display: flex; flex-wrap: wrap; gap: 8px; button { min-height: 36px; }
      @include phone { display: grid; grid-template-columns: 1fr 1fr; button { padding-inline: 8px; } }
    }
  }
  .sunline { margin: 10px 0 0; font-size: 13.5px; line-height: 1.5; }
  .note { margin: 10px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--color-muted); }
</style>
