/** Angles on the globe, shared by the calculators that work with places. */

export const RAD = Math.PI / 180;

/** A bearing or longitude brought into 0–360°. */
export const wrap360 = (degrees) => ((degrees % 360) + 360) % 360;

/** The compass point nearest a bearing, out of `points` (N, NE, E… in order, 8 or 16 of them). */
export const compassPoint = (bearing, points) =>
  points[Math.round(wrap360(bearing) / (360 / points.length)) % points.length];

/** Whether a latitude and longitude make a place. */
export const isPlace = (lat, lon) => Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180;

/** Degrees to three places, some 100 m: as close as a click on a map means. */
export const round3 = (degrees) => Math.round(degrees * 1000) / 1000;
