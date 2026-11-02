<script>
  /** The spot picked on the golden hour's map, as seen standing there: the
   *  land against the sky all the way round (lib/horizon.js), the sky in the
   *  colours of the time picked, and the sun's path for the day over it, so
   *  it shows whether the sunrise or the sunset can be seen from there or
   *  goes behind a hill first. Dragged sideways to look round. By night,
   *  the stars, the planets, the moon and the Milky Way, as many as the sky
   *  there lets through; and the clouds forecast for the next day. */
  import { untrack } from 'svelte';
  import { i18n, t, n, clock } from '$lib/i18n.svelte.js';
  import { lightOf, skyFrom, sunCoords, sunPosition } from '$lib/calc.js';
  import { atMinute, compassPoint, localDay, spotKey, wrap360 } from '$lib/geo.js';
  import { STEP, horizonFrom, landAt } from '$lib/horizon.js';
  import { CORE, FULL_DARK, bestNights, faintest, moonFrom, moonPhase, nightSky, skyBrightness } from '$lib/night.js';
  import { drawSky } from '$lib/drawsky.js';
  import { skyOrUnlit } from '$lib/darksky.js';
  import { clouds, hourOf } from '$lib/weather.js';
  import { GROW, SHRINK } from '$lib/map.js';
  import '$lib/map.css';
  import SkyDome from './SkyDome.svelte';
  import CloudStrip from './CloudStrip.svelte';
  import BestNights from './BestNights.svelte';

  /** `facing` and `shown`: which way the view looks, and how wide it is, in
   *  degrees; bound, so the map can show them. `minutes`: the time picked,
   *  from midnight on `day`. `around`: all round, not the usual width. `dome`:
   *  the whole sky overhead, not the panorama. `opened`: the spotKey this was
   *  last opened on, so coming back or following a link leaves all that as it
   *  is. `link`: the address's hash for all of this, to share. */
  let {
    lat, lon, day = $bindable(), light, link, opened = $bindable(), minutes = $bindable(),
    facing = $bindable(), around = $bindable(), dome = $bindable(), shown = $bindable(),
  } = $props();
  const fov = $derived(around ? 360 : 100);

  /** The sun's half-width in degrees, its colour, and how far it may be out of view and still drawn. */
  const SUN_RADIUS = 0.27;
  const SUN = '#fff4d6';
  const MARGIN = 30;

  let status = $state('looking');
  let land = $state.raw(null);
  let width = $state(0);
  let height = $state(0);
  let canvas = $state(null);
  /** Counted once a new canvas reaches its first frame, to draw it again there. */
  let framed = $state(0);
  $effect(() => {
    if (!canvas) return;
    const frame = requestAnimationFrame(() => framed++);
    return () => cancelAnimationFrame(frame);
  });
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
  const at = (m) => atMinute(day, m);
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
    const key = spotKey(lat, lon, day);
    if (untrack(() => opened) === key) return;
    opened = key;
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
  let noCatalogue = $state(false);
  $effect(() => {
    if (nightly && !data && !noCatalogue) import('$lib/sky-data.js').then((loaded) => (data = loaded), () => (noCatalogue = true));
  });
  const nightly = $derived(dark > 0 || dome);
  $effect(() => {
    if (!nightly) return;
    pollution = null;
    return forSpot(skyOrUnlit, (found) => (pollution = found));
  });

  /** The clouds over the next days. */
  let forecast = $state.raw(null);
  let noForecast = $state(false);
  $effect(() => {
    forecast = null;
    noForecast = false;
    return forSpot(clouds, (hours) => (forecast = hours), () => (noForecast = true));
  });
  const cloudNow = $derived(hourOf(forecast, at(minutes)));
  /** The forecast's hours along the slider. */
  const cloudHours = $derived(forecast?.filter((h) => h.at >= midnight && h.at - midnight < END * 60e3) ?? null);
  const today = localDay(new Date());

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
        moon: moonFrom(look, date),
        core: look(CORE.ra, CORE.dec),
      });
    }
    return out;
  });

  /** When the sky is fully dark that night, the sun 18° down. */
  const fullDark = $derived.by(() => {
    const black = overnight.filter((p) => p.sun < FULL_DARK);
    return black.length ? { from: black[0].m, until: black.at(-1).m } : null;
  });

  /** The moon that night: when it rises and sets, and how much of it is lit. */
  const moonNight = $derived.by(() => {
    // By its top edge.
    const up = (p) => p.moon.altitude + 0.27 > 0;
    let rise = null;
    let set = null;
    for (let i = 1; i < overnight.length; i++) {
      const was = up(overnight[i - 1]);
      const is = up(overnight[i]);
      if (is && !was) rise ??= overnight[i].m;
      if (was && !is) set ??= overnight[i].m;
    }
    const middle = at(fullDark ? (fullDark.from + fullDark.until) / 2 : 1440);
    return { rise, set, up: up(overnight[0]), ...moonPhase(middle) };
  });

  /** The galaxy's core while it is dark: when it stands highest above the land, and how high. */
  const core = $derived.by(() => {
    let best = null;
    for (const { m, sun: down, core: p } of overnight) {
      if (down >= FULL_DARK || p.altitude <= (skyline ? landAt(skyline, p.bearing) : 0)) continue;
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

  /** The coming month's nights at this spot, the best marked, and going to one on a click. */
  const nights = $derived(bestNights({
    lat, lon, first: today, clear: above, cloudAt: (date) => hourOf(forecast, date)?.total,
  }));
  function pickNight(night) {
    if (night.from != null) {
      opened = spotKey(lat, lon, night.day);
      minutes = Math.min(END - 1, night.from);
      facing = night.core?.bearing ?? (lat >= 0 ? 180 : 0);
    }
    day = night.day;
  }

  /** The moon every five minutes, from noon the day before to noon two days on. */
  const moonTrack = $derived(Array.from({ length: 865 }, (_, i) => {
    const m = -720 + i * 5;
    const date = at(m);
    return { m, ...moonFrom(skyFrom(date, lat, lon), date) };
  }));
  /** The moon's pass across the sky, rise to set: the one under way at the time picked, or else the next. */
  const moonPass = $derived.by(() => {
    const up = (i) => moonTrack[i]?.altitude > -1;
    let from = Math.round((minutes + 720) / 5);
    while (from < moonTrack.length && !up(from)) from++;
    let to = from;
    while (up(from - 1)) from--;
    while (up(to + 1)) to++;
    return moonTrack.slice(from, to + 1);
  });

  /** Where the moon is at the time picked. */
  const moonNow = $derived(moonFrom(skyFrom(at(minutes), lat, lon), at(minutes)));

  /** This device's time zone, as every time here is in it. */
  const ZONE = Intl.DateTimeFormat().resolvedOptions().timeZone.replaceAll('_', ' ');
  const zone = $derived.by(() => {
    const east = -at(minutes).getTimezoneOffset();
    const whole = Math.floor(Math.abs(east) / 60);
    const part = Math.abs(east) % 60;
    const offset = `UTC${east < 0 ? '−' : '+'}${whole}${part ? `:${String(part).padStart(2, '0')}` : ''}`;
    return `${ZONE}, ${offset}`;
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
    void framed;
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

    // The moon's pass, dotted, fainter behind the land.
    g.lineWidth = 2;
    g.setLineDash([1, 6]);
    for (const behind of [true, false]) {
      g.strokeStyle = `rgba(214,224,246,${behind ? 0.3 : 0.9})`;
      g.beginPath();
      for (let i = 1; i < moonPass.length; i++) {
        const a = moonPass[i - 1];
        const b = moonPass[i];
        const xa = xOf(a.bearing);
        const xb = xOf(b.bearing);
        if (Math.max(xa, xb) < 0 || Math.min(xa, xb) > width || Math.abs(xa - xb) > width / 2 || above(b) === behind) continue;
        g.moveTo(xa, yOf(a.altitude));
        g.lineTo(xb, yOf(b.altitude));
      }
      g.stroke();
    }
    g.setLineDash([]);
    if (!sky || dark <= 0) {
      const mx = xOf(moonNow.bearing);
      if (moonNow.altitude > -1 && mx > 0 && mx < width) {
        g.beginPath();
        g.arc(mx, yOf(moonNow.altitude), 6, 0, 2 * Math.PI);
        g.stroke();
      }
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
      g.fillText(hm(p.m), x, y - 9);
    }

    // The moon's hours, every other one, and its name where it stands highest.
    g.fillStyle = 'rgb(214,224,246)';
    let topmost = null;
    for (const moon of moonPass) {
      const x = xOf(moon.bearing);
      if (moon.altitude < 0 || x < 0 || x > width) continue;
      if (!topmost || moon.altitude > topmost.altitude) topmost = { x, altitude: moon.altitude };
      if (moon.m % 120) continue;
      g.beginPath();
      g.arc(x, yOf(moon.altitude), 2, 0, 2 * Math.PI);
      g.fill();
      g.fillText(hm(moon.m), x, yOf(moon.altitude) + 16);
    }
    if (topmost) g.fillText(t('calc.sun.view.moon'), topmost.x, yOf(topmost.altitude) - 10);
    g.fillStyle = '#fff';

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
  /** Through the device's own sharing where it has one, and to the clipboard where not. */
  let shared = $state('');
  let sharedWait = null;
  async function share() {
    const url = location.origin + location.pathname + link;
    if (navigator.share) {
      try {
        await navigator.share({ title: document.title, url });
        return;
      } catch (error) {
        if (error.name === 'AbortError') return;
      }
    }
    try {
      await navigator.clipboard.writeText(url);
      shared = 'copied';
    } catch {
      shared = 'failed';
    }
    clearTimeout(sharedWait);
    sharedWait = setTimeout(() => (shared = ''), 4000);
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
  <!-- Which view: the horizon from the spot, or the whole sky overhead. -->
  <div class="mode" role="group" aria-label={t('calc.sun.view.mode')}>
    <button type="button" aria-pressed={!dome} onclick={() => (dome = false)}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 18l6-7 4 4 3-3 7 6M2 21h20"/></svg>
      {t('calc.sun.view.horizon')}
    </button>
    <button type="button" aria-pressed={dome} onclick={() => (dome = true)}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9.5"/><path d="M12 6.5l1.2 2.6 2.8.3-2.1 1.9.6 2.8L12 12.7l-2.5 1.4.6-2.8L8 9.4l2.8-.3zM7 16.5h.01M17 16h.01M16.5 7h.01"/></svg>
      {t('calc.sun.view.starMap')}
    </button>
  </div>
  {#if cloudNow}
    <p class="cloudy">{t('calc.sun.clouds.now', { total: cloudNow.total, low: cloudNow.low })}</p>
  {/if}
  <button type="button" class="map-grow grow" aria-label={t(tall ? 'calc.view.smaller' : 'calc.view.bigger')}
          title={t(tall ? 'calc.view.smaller' : 'calc.view.bigger')} onclick={() => (tall = !tall)}>
    {@html tall ? SHRINK : GROW}
  </button>
  {#if noCatalogue}<p class="status">{t('calc.sun.night.noCatalogue')}</p>
  {:else if status === 'looking'}<p class="status">{t('calc.sun.view.looking')}</p>{/if}
</div>

<div class="time">
  <label>
    <span>{t('calc.sun.view.time')} <strong>{hm(minutes)}</strong>{minutes >= 1440 ? ` ${t('calc.sun.view.nextDay')}` : ''} · {zone}</span>
    <!-- The track is the day's light, from night to night. -->
    <input type="range" min="0" max={END - 1} step="1" bind:value={minutes} style:background-image={track} />
  </label>
  <div class="go">
    <button type="button" class="ghost" disabled={!light?.sunrise} onclick={() => goTo(light.sunrise)}>{t('calc.sun.view.sunrise')}</button>
    <button type="button" class="ghost" disabled={!light?.sunset} onclick={() => goTo(light.sunset)}>{t('calc.sun.view.sunset')}</button>
    <button type="button" class="ghost" disabled={!fullDark} onclick={tonight}>{t('calc.sun.night.tonight')}</button>
    <button type="button" class="ghost" aria-pressed={around} onclick={() => (around = !around)}>{t('calc.sun.view.around')}</button>
    <button type="button" class="ghost" onclick={share}>{t(shared === 'copied' ? 'calc.sun.view.copied' : 'calc.sun.view.share')}</button>
  </div>
</div>
{#if shared === 'failed'}<p class="sunline" role="status">{t('calc.sun.view.shareFailed')}</p>{/if}
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
<CloudStrip {lat} {lon} hours={cloudHours} failed={noForecast} picked={cloudNow} far={midnight - new Date(`${today}T00:00`) > 7 * 864e5} />
<BestNights {nights} {day} onpick={pickNight} />
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
  .view.dome {
    height: calc(min(560px, 92vw) + 44px);
    /* The chart is round: the corners under it are free. */
    .cloudy { top: auto; bottom: 8px; }
  }
  .view.tall { height: min(82vh, 960px); }
  /* Drawn as the maps' is (map.css). */
  .grow { position: absolute; top: 8px; right: 8px; border-radius: 4px; box-shadow: 0 1px 5px rgba(0, 0, 0, .4); }
  .mode {
    position: absolute; top: 8px; left: 8px; display: flex; padding: 3px; gap: 2px; border-radius: 9px;
    background: rgba(10, 14, 28, .72); box-shadow: 0 1px 5px rgba(0, 0, 0, .4);
    button {
      display: flex; align-items: center; gap: 6px; min-height: 32px; padding: 4px 11px; border: 0; border-radius: 7px;
      font: 600 12.5px var(--font-sans, system-ui, sans-serif); color: rgba(255, 255, 255, .78);
      background: transparent; box-shadow: none; cursor: pointer;
      &:hover { color: #fff; }
      &[aria-pressed='true'] { color: #10131c; background: #f2c46a; }
      &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 1px; }
    }
    svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  }
  .cloudy {
    position: absolute; top: 54px; left: 8px; margin: 0; padding: 3px 8px; border-radius: 6px;
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
