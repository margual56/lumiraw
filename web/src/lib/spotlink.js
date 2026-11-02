/** A golden hour's spot, day, time and view packed into a short code, for
 *  sharing: `/sky#1aZ09xQkLm2Pz0w`. Nothing is stored anywhere: the code is the
 *  settings, and it sits after the `#`, so it does not reach the site's host. */
import { round3, wrap360 } from './geo.js';

const ALPHABET = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const VERSION = '1';
const LENGTH = 14;
const CODE = new RegExp(`^${VERSION}[0-9A-Za-z]{${LENGTH}}$`);
/** The last minute on the slider, noon the next day. */
const LAST_MINUTE = 2159;
/** How many values each field takes, in the order packed. */
const RADIX = { lat: 180001, lon: 360001, day: 65536, minutes: 2160, tz: 105, facing: 360, around: 2, dome: 2 };

const eastOfUtc = (day) => -new Date(`${day}T12:00`).getTimezoneOffset();
const dayNumber = (day) => Date.parse(`${day}T00:00Z`) / 864e5 - 10957;
const dayName = (number) => new Date((number + 10957) * 864e5).toISOString().slice(0, 10);

/** The address's hash for these settings. */
export function writeLink({ lat, lon, day, minutes, facing, around, dome }) {
  const fields = {
    lat: Math.round((lat + 90) * 1000),
    lon: Math.round((lon + 180) * 1000),
    day: dayNumber(day),
    minutes: Math.round(minutes),
    tz: Math.round(eastOfUtc(day) / 15) + 48,
    facing: wrap360(Math.round(facing)),
    around: around ? 1 : 0,
    dome: dome ? 1 : 0,
  };
  let packed = 0n;
  for (const [name, radix] of Object.entries(RADIX)) packed = packed * BigInt(radix) + BigInt(fields[name]);
  let code = '';
  for (let i = 0; i < LENGTH; i++) {
    code = ALPHABET[Number(packed % 62n)] + code;
    packed /= 62n;
  }
  return `#${VERSION}${code}`;
}

/** What a hash asks for: `tool`, and with a code the settings it holds. */
export function readLink(hash) {
  const asked = hash.replace(/^#/, '');
  if (!CODE.test(asked)) return { tool: asked };
  let packed = 0n;
  for (const letter of asked.slice(VERSION.length)) packed = packed * 62n + BigInt(ALPHABET.indexOf(letter));
  const fields = {};
  for (const [name, radix] of Object.entries(RADIX).reverse()) {
    fields[name] = Number(packed % BigInt(radix));
    packed /= BigInt(radix);
  }
  if (packed !== 0n) return { tool: 'sun' };
  const day = dayName(fields.day);
  // The same moment in the time zone it is read in.
  const shift = eastOfUtc(day) - (fields.tz - 48) * 15;
  return {
    tool: 'sun',
    lat: round3(fields.lat / 1000 - 90),
    lon: round3(fields.lon / 1000 - 180),
    day,
    minutes: Math.max(0, Math.min(LAST_MINUTE, fields.minutes + shift)),
    facing: fields.facing,
    around: fields.around === 1,
    dome: fields.dome === 1,
  };
}
