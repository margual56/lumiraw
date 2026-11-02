/** The clouds over a place for the next 16 days, hour by hour, from Open-Meteo
 *  (https://open-meteo.com, CC BY 4.0), which takes no account or key and
 *  mixes the national weather services' models for wherever it is asked. */

import { round3 } from './geo.js';
import { remoteFetch } from './remote.js';

const API = 'https://api.open-meteo.com/v1/forecast';
export const WEATHER_PAGE = 'https://open-meteo.com/';
export const DAYS = 16;

/** Forecasts kept by place, so going back to one is not asked again; an hour is as long as one lasts. */
const cache = new Map();
const FRESH = 3600e3;

/**
 * Every hour of the next `DAYS` days, each `{ at, total, low, mid, high }`:
 * when it starts (a Date) and the share of the sky covered (0–100) by all the
 * cloud and by the low (to 3 km), middle and high (from 8 km) layers.
 */
export function clouds(lat, lon) {
  // No closer than the map's click: some 100 m.
  const key = `${round3(lat)},${round3(lon)}`;
  const kept = cache.get(key);
  if (kept && Date.now() - kept.asked < FRESH) return kept.loading;
  const loading = (async () => {
    const response = await remoteFetch(`${API}?latitude=${round3(lat)}&longitude=${round3(lon)}&forecast_days=${DAYS}&timeformat=unixtime`
      + '&hourly=cloud_cover,cloud_cover_low,cloud_cover_mid,cloud_cover_high');
    if (!response.ok) throw new Error(String(response.status));
    const { hourly } = await response.json();
    return hourly.time.map((time, i) => ({
      at: new Date(time * 1000),
      total: hourly.cloud_cover[i],
      low: hourly.cloud_cover_low[i],
      mid: hourly.cloud_cover_mid[i],
      high: hourly.cloud_cover_high[i],
    }));
  })();
  // A failure is not kept, so the next ask tries again.
  loading.catch(() => cache.delete(key));
  cache.set(key, { asked: Date.now(), loading });
  return loading;
}

/** The hour of a forecast `date` falls in, or null outside it. */
export const hourOf = (hours, date) => (hours?.length ? hours[Math.floor((date - hours[0].at) / 3600e3)] ?? null : null);
