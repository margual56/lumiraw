/** The calculators' maps: Leaflet over OpenStreetMap's tiles, loaded the
 *  first time one opens. */
import { round3 } from './geo.js';

/**
 * A map in `element` at `centre` and `zoom`, calling `onpick(lat, lon)` for
 * a click, with a button in its corner that makes it much taller and back,
 * labelled by `bigger` (`{ more, less }`). Gives `{ leaflet, map }`, for the
 * caller to add its own layers and to remove the map when done.
 */
export async function openMap(element, { centre, zoom, onpick, bigger }) {
  const leaflet = (await import('leaflet')).default;
  await import('leaflet/dist/leaflet.css');
  await import('./map.css');
  const map = leaflet.map(element, { worldCopyJump: true }).setView(centre, zoom);
  leaflet.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 18,
    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
  }).addTo(map);
  map.on('click', (e) => {
    const p = e.latlng.wrap();
    onpick(round3(p.lat), round3(p.lng));
  });
  const Grow = leaflet.Control.extend({
    onAdd() {
      const button = leaflet.DomUtil.create('button', 'map-grow leaflet-bar');
      button.type = 'button';
      const label = () => {
        button.textContent = element.classList.contains('big') ? bigger.less : bigger.more;
        button.setAttribute('aria-pressed', String(element.classList.contains('big')));
      };
      label();
      leaflet.DomEvent.disableClickPropagation(button);
      leaflet.DomEvent.on(button, 'click', () => {
        const middle = map.getCenter();
        element.classList.toggle('big');
        label();
        // Leaflet reads its size once; told again, keeping the same middle.
        map.invalidateSize({ pan: false });
        map.setView(middle, map.getZoom(), { animate: false });
        element.scrollIntoView({ block: 'nearest' });
      });
      return button;
    },
  });
  if (bigger) new Grow({ position: 'topright' }).addTo(map);
  return { leaflet, map };
}

/** The spot picked, as a dot. */
export const spotMarker = (leaflet, at) =>
  leaflet.circleMarker(at, { radius: 8, weight: 3, color: '#fff', fillColor: '#e8a33d', fillOpacity: 1 });
