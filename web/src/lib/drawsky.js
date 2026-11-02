/** The night sky drawn on a canvas, for both of the golden hour's views: the
 *  panorama and the whole sky. Each view says where a bearing and altitude
 *  fall on it; the rest is the same. */

import { RAD } from './geo.js';

/** A star's colour by its B−V index: blue-white to orange. */
const COLOURS = [[-0.3, [170, 191, 255]], [0, [205, 218, 255]], [0.6, [255, 246, 236]], [1, [255, 222, 182]], [1.6, [255, 188, 128]]];
function colourOf(bv) {
  const i = COLOURS.findIndex(([b]) => b > bv);
  if (i <= 0) return COLOURS[i === 0 ? 0 : COLOURS.length - 1][1];
  const [b0, c0] = COLOURS[i - 1];
  const [b1, c1] = COLOURS[i];
  const k = (bv - b0) / (b1 - b0);
  return c0.map((x, j) => Math.round(x + (c1[j] - x) * k));
}

/** How big a star is drawn by its magnitude, in pixels across from the centre. */
const radiusOf = (mag) => Math.min(3.6, Math.max(0.55, 2.8 - 0.42 * (mag + 1)));

/** The point `degrees` along the way from one place on the sky to another, both as bearing and altitude. */
function toward(from, to, degrees) {
  const vector = ({ bearing, altitude }) => [
    Math.cos(altitude * RAD) * Math.sin(bearing * RAD), Math.cos(altitude * RAD) * Math.cos(bearing * RAD), Math.sin(altitude * RAD)];
  const a = vector(from);
  const b = vector(to);
  const angle = Math.acos(Math.max(-1, Math.min(1, a[0] * b[0] + a[1] * b[1] + a[2] * b[2])));
  if (angle < 1e-6) return from;
  const k = Math.min(1, (degrees * RAD) / angle);
  const p = a.map((x, i) => x * Math.sin((1 - k) * angle) / Math.sin(angle) + b[i] * Math.sin(k * angle) / Math.sin(angle));
  return { bearing: Math.atan2(p[0], p[1]) / RAD, altitude: Math.asin(Math.max(-1, Math.min(1, p[2]))) / RAD };
}

let haze = null;

/**
 * Draw `sky` (night.js's `nightSky`) on the context `g`, `width` by `height`:
 * `place(bearing, altitude)` gives the point on the canvas, or null where it
 * is out of view, and `scale` its pixels a degree; `broken(a, b)` says where
 * a line between two points jumps across the view and is not drawn. `limit`
 * is the faintest magnitude seen, `dark` how far into the night it is (0
 * daylight to 1 full night), `milky` how plainly the Milky Way shows (0–1),
 * `ranks` the constellations named (1 the best known only, to 3 all), and
 * `sun` where the sun is, which the moon's lit side faces. `planetName`
 * gives a planet's name in the page's language.
 */
export function drawSky(g, sky, { width, height, place, broken, scale, limit, dark, milky, ranks, sun, planetName }) {
  if (dark <= 0) return;
  g.save();

  // The Milky Way: its cells drawn small and enlarged, which blurs them into a glow.
  if (milky > 0) {
    const shrink = 6;
    const w = Math.ceil(width / shrink);
    const h = Math.ceil(height / shrink);
    haze ??= document.createElement('canvas');
    if (haze.width !== w) haze.width = w;
    if (haze.height !== h) haze.height = h;
    const m = haze.getContext('2d');
    m.clearRect(0, 0, w, h);
    m.globalCompositeOperation = 'lighter';
    const r = Math.max(1.5, (3.2 * scale) / shrink);
    for (const cell of sky.milky) {
      const at = place(cell.bearing, cell.altitude);
      if (!at) continue;
      // Cells are closer together towards the poles; each counts for less.
      m.fillStyle = `rgba(190,204,255,${0.028 * cell.level * Math.cos(cell.dec * RAD)})`;
      m.beginPath();
      m.arc(at[0] / shrink, at[1] / shrink, r, 0, 2 * Math.PI);
      m.fill();
    }
    g.globalAlpha = Math.min(1, milky * dark);
    g.globalCompositeOperation = 'lighter';
    g.imageSmoothingEnabled = true;
    g.drawImage(haze, 0, 0, w * shrink, h * shrink);
    g.globalCompositeOperation = 'source-over';
    g.globalAlpha = 1;
  }

  // The constellations' lines.
  g.strokeStyle = `rgba(140,170,240,${0.38 * dark})`;
  g.lineWidth = 1;
  g.beginPath();
  for (const run of sky.lines) {
    let last = null;
    for (let k = 0; k < run.length; k += 2) {
      const at = run[k + 1] > -3 ? place(run[k], run[k + 1]) : null;
      if (at && last && !broken(last, at)) g.lineTo(at[0], at[1]);
      else if (at) g.moveTo(at[0], at[1]);
      last = at;
    }
  }
  g.stroke();

  // The stars, faintest first so the bright ones sit on top.
  for (let i = sky.stars.length - 4; i >= 0; i -= 4) {
    const mag = sky.stars[i + 2];
    const alpha = Math.min(1, (limit - mag) / 1.2) * dark;
    if (alpha <= 0.02) continue;
    const at = place(sky.stars[i], sky.stars[i + 1]);
    if (!at) continue;
    const r = radiusOf(mag);
    g.fillStyle = `rgba(${colourOf(sky.stars[i + 3])},${alpha})`;
    if (r < 1) g.fillRect(at[0] - r, at[1] - r, 2 * r, 2 * r);
    else {
      g.beginPath();
      g.arc(at[0], at[1], r, 0, 2 * Math.PI);
      g.fill();
    }
  }

  g.font = '600 11px system-ui, sans-serif';
  g.textAlign = 'center';
  g.fillStyle = `rgba(176,196,246,${0.8 * dark})`;
  for (const c of sky.labels) {
    if (c.rank > ranks || c.altitude < 4) continue;
    const at = place(c.bearing, c.altitude);
    if (at) g.fillText(c.name.toUpperCase(), at[0], at[1]);
  }
  g.font = '500 11px system-ui, sans-serif';
  g.textAlign = 'left';
  g.fillStyle = `rgba(255,255,255,${0.75 * dark})`;
  for (const s of sky.names) {
    if (s.altitude < 2 || s.mag > limit - 0.5) continue;
    const at = place(s.bearing, s.altitude);
    if (at) g.fillText(s.name, at[0] + 5, at[1] - 4);
  }

  // The planets: steady, and warmer than the stars about them.
  for (const p of sky.planets) {
    if (p.altitude < -1 || p.mag > limit) continue;
    const at = place(p.bearing, p.altitude);
    if (!at) continue;
    g.fillStyle = `rgba(255,227,166,${Math.min(1, (limit - p.mag) / 1.2) * dark})`;
    g.beginPath();
    g.arc(at[0], at[1], radiusOf(p.mag) + 0.6, 0, 2 * Math.PI);
    g.fill();
    g.fillText(planetName(p.name), at[0] + 6, at[1] + 12);
  }
  g.restore();

  // The moon, its lit side towards the sun, whatever the time.
  const moon = sky.moon;
  const at = moon.altitude > -1 && place(moon.bearing, moon.altitude);
  if (!at) return;
  const ahead = toward(moon, sun, 2);
  const step = place(ahead.bearing, ahead.altitude);
  const facing = step ? Math.atan2(step[1] - at[1], step[0] - at[0]) : 0;
  const r = Math.max(6, 0.26 * scale);
  g.save();
  const glow = g.createRadialGradient(at[0], at[1], r, at[0], at[1], r * 6);
  glow.addColorStop(0, `rgba(230,236,255,${0.3 * moon.lit})`);
  glow.addColorStop(1, 'rgba(230,236,255,0)');
  g.fillStyle = glow;
  g.beginPath();
  g.arc(at[0], at[1], r * 6, 0, 2 * Math.PI);
  g.fill();
  // The unlit part, faintly, by the earth's light.
  g.fillStyle = 'rgba(200,206,224,.16)';
  g.beginPath();
  g.arc(at[0], at[1], r, 0, 2 * Math.PI);
  g.fill();
  g.fillStyle = '#f2f0e6';
  g.beginPath();
  g.arc(at[0], at[1], r, facing - Math.PI / 2, facing + Math.PI / 2);
  // Back along the terminator: bulging to the lit side for a crescent, the dark side past half.
  g.ellipse(at[0], at[1], r * Math.abs(1 - 2 * moon.lit), r, facing, Math.PI / 2, -Math.PI / 2, moon.lit < 0.5);
  g.fill();
  g.restore();
}
