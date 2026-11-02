/** The calculators' maps: Leaflet over OpenStreetMap's tiles, loaded the
 *  first time one opens. */
import { round3 } from './geo.js';
import { remoteFetch } from './remote.js';

/** The icons on the buttons that make a map or a view taller and back: arrows out to the corners, and in. */
const ARROWS = (d) => `<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>`;
export const GROW = ARROWS('M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7');
export const SHRINK = ARROWS('M20 10h-6V4M4 14h6v6M14 10l7-7M10 14l-7 7');

/**
 * A map in `element` at `centre` and `zoom`, calling `onpick(lat, lon)` for
 * a click, with a button in its corner that makes it much taller and back,
 * named for screen readers and on hover by what `bigger` gives (`{ more, less }`).
 * Gives `{ leaflet, map, relabel }`, for the caller to add its own layers, to
 * name the button again when the language changes and to remove the map when done.
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
  let relabel = () => {};
  const Grow = leaflet.Control.extend({
    onAdd() {
      const button = leaflet.DomUtil.create('button', 'map-grow leaflet-bar');
      button.type = 'button';
      const label = () => {
        const big = element.classList.contains('big');
        button.innerHTML = big ? SHRINK : GROW;
        button.title = big ? bigger().less : bigger().more;
        button.setAttribute('aria-label', button.title);
      };
      label();
      relabel = label;
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
  return { leaflet, map, relabel };
}

/** A layer of tiles that come through `remoteFetch`, so that the desktop app keeps them too. */
export function keptTiles(leaflet, url, options) {
  const Kept = leaflet.TileLayer.extend({
    createTile(coords, done) {
      const tile = document.createElement('img');
      tile.alt = '';
      remoteFetch(this.getTileUrl(coords))
        .then((response) => (response.ok ? response.blob() : Promise.reject(new Error(String(response.status)))))
        .then((blob) => {
          const src = URL.createObjectURL(blob);
          tile.onload = () => { URL.revokeObjectURL(src); done(null, tile); };
          tile.onerror = (error) => { URL.revokeObjectURL(src); done(error, tile); };
          tile.src = src;
        })
        .catch((error) => done(error, tile));
      return tile;
    },
  });
  return new Kept(url, options);
}

/** The spot picked, as a dot. */
export const spotMarker = (leaflet, at) =>
  leaflet.circleMarker(at, { radius: 8, weight: 3, color: '#fff', fillColor: '#e8a33d', fillOpacity: 1 });
