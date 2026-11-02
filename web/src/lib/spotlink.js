/** A golden hour's spot, day and time in the address, for sharing:
 *  `#sun&lat=40.4&lon=-3.7&date=2026-11-02&time=18:30&tz=60`. After the `#`,
 *  so the spot does not reach the site's host when the link is opened. `time`
 *  is as on the slider, so past 24:00 is the next morning; `tz` is the minutes
 *  east of UTC it was read in, so the link means the same moment anywhere. */
import { isPlace } from './geo.js';

/** The last minute on the slider, noon the next day. */
export const LAST_MINUTE = 2159;

const eastOfUtc = (day) => -new Date(`${day}T12:00`).getTimezoneOffset();
const two = (x) => String(x).padStart(2, '0');

/** The address's hash for this spot, day and minute. */
export function writeLink(tool, { lat, lon, day, minutes }) {
  const time = `${two(Math.floor(minutes / 60))}:${two(minutes % 60)}`;
  return `#${tool}&lat=${lat}&lon=${lon}&date=${day}&time=${time}&tz=${eastOfUtc(day)}`;
}

/** What a hash asks for: `tool`, and `lat`, `lon`, `day` and `minutes` where given and sound. */
export function readLink(hash) {
  const [tool, ...rest] = hash.replace(/^#/, '').split('&');
  const asked = new URLSearchParams(rest.join('&'));
  const out = { tool };

  const lat = Number(asked.get('lat'));
  const lon = Number(asked.get('lon'));
  if (asked.get('lat') && asked.get('lon') && isPlace(lat, lon)) Object.assign(out, { lat, lon });

  const day = asked.get('date') ?? '';
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day) || Number.isNaN(new Date(`${day}T12:00`).valueOf())) return out;
  out.day = day;

  const time = /^(\d{1,2}):(\d{2})$/.exec(asked.get('time') ?? '');
  if (time) {
    const tz = Number(asked.get('tz'));
    const shift = asked.get('tz') && Number.isFinite(tz) ? eastOfUtc(day) - tz : 0;
    out.minutes = Math.max(0, Math.min(LAST_MINUTE, Number(time[1]) * 60 + Number(time[2]) + shift));
  }
  return out;
}
