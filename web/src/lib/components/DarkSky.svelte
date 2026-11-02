<script>
  /** Dark skies: the light pollution atlas over OpenStreetMap, how dark the
   *  sky is at the spot picked, and the darkest places on land within a
   *  distance of it, for a night of stars (lib/darksky.js). The spot is the
   *  same as the golden hour's. */
  import { untrack } from 'svelte';
  import { t, n } from '$lib/i18n.svelte.js';
  import { ATLAS_PAGE, BORTLE_PAGE, OVERLAY, between, darkSpots, skyAt } from '$lib/darksky.js';
  import { heightsAround } from '$lib/horizon.js';

  /** `onpick`: a spot's golden hour asked for. */
  let { lat = $bindable(), lon = $bindable(), locate, locating, onpick } = $props();

  const placed = $derived(Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180);
  const round3 = (x) => Math.round(x * 1000) / 1000;

  let km = $state(50);
  let here = $state.raw(null);
  let hereStatus = $state('');
  let spots = $state.raw([]);
  let status = $state('');

  // -- the map --------------------------------------------------------------------

  let element = $state(null);
  let map = null;
  let leaflet = null;
  let marker = null;
  let circle = null;
  let pins = [];
  /** A spot put by clicking this map is in view already; any other is flown to. */
  let clicked = false;

  $effect(() => {
    if (!element) return;
    let gone = false;
    (async () => {
      leaflet = (await import('leaflet')).default;
      await import('leaflet/dist/leaflet.css');
      if (gone) return;
      map = leaflet.map(element, { worldCopyJump: true }).setView(placed ? [lat, lon] : [40, -3.7], placed ? 8 : 5);
      leaflet.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 18,
        attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
      }).addTo(map);
      leaflet.tileLayer(OVERLAY, {
        minZoom: 2, maxNativeZoom: 8, maxZoom: 18, tileSize: 1024, zoomOffset: -2, opacity: 0.5,
        attribution: `<a href="${ATLAS_PAGE}">Light Pollution Atlas</a> &copy; David Lorenz`,
      }).addTo(map);
      map.on('click', (e) => {
        const p = e.latlng.wrap();
        clicked = true;
        lat = round3(p.lat);
        lon = round3(p.lng);
      });
      draw();
    })();
    return () => {
      gone = true;
      map?.remove();
      map = null;
      marker = circle = null;
      pins = [];
    };
  });

  /** The spot, the circle searched, and the spots found, on the map. */
  function draw() {
    if (!map || !leaflet) return;
    marker?.remove();
    circle?.remove();
    marker = circle = null;
    if (placed) {
      circle = leaflet.circle([lat, lon], {
        radius: km * 1000, color: '#fff', weight: 2, opacity: 0.9, fill: false, dashArray: '8 6', interactive: false,
      }).addTo(map);
      marker = leaflet.circleMarker([lat, lon], {
        radius: 8, weight: 3, color: '#fff', fillColor: '#e8a33d', fillOpacity: 1,
      }).addTo(map);
      if (!clicked) map.fitBounds(circle.getBounds(), { maxZoom: 10 });
    }
    clicked = false;
    for (const pin of pins) pin.remove();
    pins = spots.map((spot, i) => leaflet.marker([spot.lat, spot.lon], {
      icon: leaflet.divIcon({ className: 'dark-pin', html: `<span>${i + 1}</span>`, iconSize: [26, 26] }),
      keyboard: false,
    }).addTo(map));
  }

  // A new spot or distance: its sky read again, the old spots gone.
  $effect(() => {
    void km;
    if (!placed) {
      here = null;
      spots = [];
      untrack(draw);
      return;
    }
    const spot = [lat, lon];
    spots = [];
    status = '';
    hereStatus = 'reading';
    skyAt(...spot).then((sky) => {
      if (spot[0] !== lat || spot[1] !== lon) return;
      here = sky;
      hereStatus = sky ? '' : 'outside';
    }, () => {
      hereStatus = 'failed';
    });
    untrack(draw);
  });

  let searched = 0;
  async function search() {
    const job = ++searched;
    status = 'searching';
    spots = [];
    try {
      // The land's heights, to leave out the sea and give each spot its height.
      const zoom = km > 60 ? 8 : 9;
      const height = await heightsAround(lat, lon, [[km * 1000, zoom]]);
      const found = await darkSpots(lat, lon, km, (la, lo) => (height(la, lo, zoom) ?? 0) > 1);
      if (job !== searched) return;
      spots = found.map((spot) => ({ ...spot, ...between(lat, lon, spot.lat, spot.lon), height: height(spot.lat, spot.lon, zoom) }));
      status = spots.length ? '' : 'none';
      draw();
      if (spots.length) map?.fitBounds(circle.getBounds(), { maxZoom: 10 });
    } catch {
      if (job === searched) status = 'failed';
    }
  }

  const POINTS = $derived(t('calc.sun.points').split(','));
  const side = (bearing) => POINTS[Math.round(bearing / 45) % 8];
  const colour = (c) => `rgb(${c.join(',')})`;
  const directions = (spot) => `https://www.google.com/maps/dir/?api=1&destination=${spot.lat.toFixed(4)},${spot.lon.toFixed(4)}`;
  function show(spot) {
    map?.setView([spot.lat, spot.lon], 11);
    element?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }
</script>

<div class="map" bind:this={element}></div>
<p class="note">{t('calc.dark.mapNote')}</p>

<div class="legend" aria-hidden="true">
  {#each [['0', [0, 0, 0]], ['1', [66, 66, 66]], ['2', [33, 84, 216]], ['3', [31, 161, 42]], ['4', [184, 166, 37]], ['5', [253, 150, 80]], ['6', [251, 90, 73]], ['7', [242, 242, 242]]] as [zone, c] (zone)}
    <span><i style:background={colour(c)}></i>{zone}</span>
  {/each}
  <em>{t('calc.dark.legend')}</em>
</div>

<div class="where">
  <span class="spot">
    {#if placed}{t('calc.dark.spot', { lat: n(lat, 3), lon: n(lon, 3) })}{:else}{t('calc.dark.ask')}{/if}
  </span>
  <button type="button" class="ghost" onclick={locate} disabled={locating}>
    {locating ? t('calc.sun.locating') : t('calc.sun.here')}
  </button>
</div>

{#if placed}
  <div class="sky">
    {#if here}
      <div class="figure">
        <span class="label">{t('calc.dark.bortle')}</span>
        <strong>≈ {here.bortle}</strong>
      </div>
      <div class="figure">
        <span class="label">{t('calc.dark.sqm')}</span>
        <strong>{n(here.sqm, 2)}</strong>
      </div>
      <div class="figure">
        <span class="label">{t('calc.dark.zone')}</span>
        <strong><i style:background={colour(here.colour)}></i>{here.zone}</strong>
      </div>
    {:else if hereStatus}
      <p class="status">{t(`calc.dark.${hereStatus}`)}</p>
    {/if}
  </div>

  <div class="search">
    <label>
      <span>{t('calc.dark.within')} <strong>{km} km</strong></span>
      <input type="range" min="5" max="150" step="5" bind:value={km} />
    </label>
    <button type="button" onclick={search} disabled={status === 'searching' || hereStatus === 'outside'}>
      {status === 'searching' ? t('calc.dark.searching') : t('calc.dark.find')}
    </button>
  </div>
  {#if status && status !== 'searching'}<p class="status">{t(`calc.dark.${status}`)}</p>{/if}

  {#if spots.length}
    <ol class="spots">
      {#each spots as spot, i (spot.lat + ',' + spot.lon)}
        <li>
          <span class="number">{i + 1}</span>
          <div class="facts">
            <strong>{t('calc.dark.away', { km: n(spot.km, 0), side: side(spot.bearing) })}</strong>
            <span>
              <i style:background={colour(spot.colour)}></i>
              {t('calc.dark.spotSky', { bortle: spot.bortle, sqm: n(spot.sqm, 2) })}{#if spot.height != null}{` · ${n(spot.height, 0)} m`}{/if}
            </span>
          </div>
          <div class="links">
            <button type="button" class="link" onclick={() => show(spot)}>{t('calc.dark.show')}</button>
            <button type="button" class="link" onclick={() => onpick(round3(spot.lat), round3(spot.lon))}>{t('calc.dark.golden')}</button>
            <a href={directions(spot)} target="_blank" rel="noopener">{t('calc.dark.directions')}</a>
          </div>
        </li>
      {/each}
    </ol>
  {/if}
{/if}
<p class="note">{t('calc.dark.note')} <a href={BORTLE_PAGE} target="_blank" rel="noopener">{t('calc.dark.why')}</a></p>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .map {
    /* Leaflet's panes kept inside, under the sticky header. */
    isolation: isolate;
    height: 420px; border-radius: 10px; overflow: hidden; background: var(--color-panel-2);
    @include phone { height: 300px; }
    :global(.leaflet-control-attribution) { font-size: 10px; }
    :global(.dark-pin) {
      display: grid; place-items: center; border-radius: 50%;
      background: var(--color-accent); color: var(--color-accent-ink);
      font: 700 13px var(--font-sans, system-ui); box-shadow: 0 0 0 2px #fff;
    }
  }
  .note { margin: 8px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--color-muted); }

  /* The atlas's colours, zone by zone. */
  .legend {
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; margin-top: 10px;
    font-size: 12px; color: var(--color-muted);
    span { display: inline-flex; align-items: center; gap: 4px; }
    i { width: 14px; height: 10px; border-radius: 2px; box-shadow: inset 0 0 0 1px var(--color-line); }
    em { font-style: normal; }
  }

  .where {
    display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px 16px;
    margin-top: 16px;
    .spot { font-size: 14px; }
    button { min-height: 38px; }
  }

  /* How dark the sky is at the spot: three figures side by side. */
  .sky {
    display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-top: 16px;
    @include phone { grid-template-columns: 1fr 1fr 1fr; gap: 8px; }
    .figure { display: grid; gap: 4px; padding: 14px 16px; border-radius: 12px; background: var(--color-panel-2); }
    .label { font-size: 12px; color: var(--color-muted); }
    strong {
      display: inline-flex; align-items: center; gap: 8px; font-size: 24px; font-weight: 650;
      color: var(--color-accent); font-variant-numeric: tabular-nums;
      @include phone { font-size: 19px; }
    }
    strong i { width: 16px; height: 16px; border-radius: 4px; box-shadow: inset 0 0 0 1px var(--color-line); }
    .status { grid-column: 1 / -1; }
  }

  .search {
    display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 12px 16px; align-items: end; margin-top: 18px;
    @include phone { grid-template-columns: 1fr; }
    label { display: grid; gap: 10px; }
    label > span { font-size: 12.5px; color: var(--color-muted); }
    strong { color: var(--color-ink); font-variant-numeric: tabular-nums; }
    input {
      appearance: none; width: 100%; height: 6px; padding: 0; border: 0; border-radius: 999px;
      background: var(--color-line); margin: 6px 0;
      &::-webkit-slider-thumb {
        appearance: none; width: 18px; height: 18px; border-radius: 50%; background: var(--color-accent);
      }
      &::-moz-range-thumb { width: 18px; height: 18px; border: 0; border-radius: 50%; background: var(--color-accent); }
    }
  }
  .status { margin: 10px 0 0; font-size: 13.5px; color: var(--color-muted); }

  .spots {
    display: grid; gap: 8px; margin: 16px 0 0; padding: 0; list-style: none;
    li {
      display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 6px 14px; align-items: center;
      padding: 12px 14px; border-radius: 12px; background: var(--color-panel-2);
      @include phone { grid-template-columns: auto minmax(0, 1fr); }
    }
    .number {
      display: grid; place-items: center; width: 26px; height: 26px; border-radius: 50%;
      background: var(--color-accent); color: var(--color-accent-ink); font-size: 13px; font-weight: 700;
    }
    .facts {
      display: grid; gap: 2px; font-size: 13.5px;
      span { color: var(--color-muted); font-size: 13px; }
      i {
        display: inline-block; width: 12px; height: 12px; margin-right: 6px; vertical-align: -1px;
        border-radius: 3px; box-shadow: inset 0 0 0 1px var(--color-line);
      }
    }
    .links {
      display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: 13px;
      @include phone { grid-column: 2; }
    }
    .link {
      min-height: 0; padding: 0; background: none; color: var(--color-accent); font-weight: 500;
      &:hover { text-decoration: underline; filter: none; }
    }
    a { color: var(--color-accent); }
  }
</style>
