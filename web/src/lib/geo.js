/** Angles on the globe, shared by the calculators that work with places. */

export const RAD = Math.PI / 180;

/** A bearing or longitude brought into 0–360°. */
export const wrap360 = (degrees) => ((degrees % 360) + 360) % 360;

/** The compass point nearest a bearing, out of `points` (N, NE, E… in order, 8 or 16 of them). */
export const compassPoint = (bearing, points) =>
  points[Math.round(wrap360(bearing) / (360 / points.length)) % points.length];

/** Whether a latitude and longitude make a place. */
export const isPlace = (lat, lon) => Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180;

/** A date as `YYYY-MM-DD` on this device's calendar. */
export const localDay = (date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;

/** Degrees to three places, some 100 m: as close as a click on a map means. */
export const round3 = (degrees) => Math.round(degrees * 1000) / 1000;

/** Kilometres between two places, and the bearing from the first to the second. */
export function between(lat, lon, lat2, lon2) {
  const phi = lat * RAD;
  const phi2 = lat2 * RAD;
  const dl = (lon2 - lon) * RAD;
  const a = Math.sin((phi2 - phi) / 2) ** 2 + Math.cos(phi) * Math.cos(phi2) * Math.sin(dl / 2) ** 2;
  const bearing = Math.atan2(Math.sin(dl) * Math.cos(phi2), Math.cos(phi) * Math.sin(phi2) - Math.sin(phi) * Math.cos(phi2) * Math.cos(dl));
  return { km: 12742 * Math.asin(Math.sqrt(a)), bearing: wrap360(bearing / RAD) };
}
