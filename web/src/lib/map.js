/** The calculators' maps: Leaflet over OpenStreetMap's tiles, loaded the
 *  first time one opens. */
import { round3 } from './geo.js';

/**
 * A map in `element` at `centre` and `zoom`, calling `onpick(lat, lon)` for
 * a click. Gives `{ leaflet, map }`, for the caller to add its own layers and
 * to remove the map when done.
 */
export async function openMap(element, { centre, zoom, onpick }) {
  const leaflet = (await import('leaflet')).default;
  await import('leaflet/dist/leaflet.css');
  const map = leaflet.map(element, { worldCopyJump: true }).setView(centre, zoom);
  leaflet.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 18,
    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
  }).addTo(map);
  map.on('click', (e) => {
    const p = e.latlng.wrap();
    onpick(round3(p.lat), round3(p.lng));
  });
  return { leaflet, map };
}

/** The spot picked, as a dot. */
export const spotMarker = (leaflet, at) =>
  leaflet.circleMarker(at, { radius: 8, weight: 3, color: '#fff', fillColor: '#e8a33d', fillOpacity: 1 });
