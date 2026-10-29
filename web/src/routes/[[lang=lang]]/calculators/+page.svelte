<script>
  /** The photographer's arithmetic, one calculator at a time, worked out as
   *  the numbers are typed: depth of field, ND filters, sharp stars, and the
   *  golden and blue hours. */
  import SiteHeader from '$lib/components/SiteHeader.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { t, n, i18n } from '$lib/i18n.svelte.js';
  import {
    SENSORS, FILTERS, SHUTTERS, confusion, depthOfField, duration, lightOfDay, ndShutter, starRules,
  } from '$lib/calc.js';
  import camerasUrl from '$lib/wasm/lenses/cameras.json?url';

  const TOOLS = ['dof', 'nd', 'stars', 'sun'];
  let tool = $state('dof');

  /** What was typed, remembered in this browser between visits. */
  const STORE = 'lumiraw.calc';
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(STORE)) ?? {};
    } catch {
      return {};
    }
  })();
  /** Every input, in one object, so remembering them is one line. */
  const v = $state({
    crop: 1, camera: '', focal: 35, aperture: 8, distance: 3, base: 1 / 60, stops: 10,
    starFocal: 20, starAperture: 2.8, megapixels: 24, lat: null, lon: null, ...saved,
  });
  let day = $state(new Date().toISOString().slice(0, 10));
  let locating = $state(false);
  let located = $state('');

  $effect(() => {
    try {
      localStorage.setItem(STORE, JSON.stringify(v));
    } catch {
      // Private window or storage refused: nothing is remembered, that is all.
    }
  });

  // The calculator shown is in the address, so a link can open any of them.
  $effect(() => {
    const asked = location.hash.slice(1);
    if (TOOLS.includes(asked)) tool = asked;
  });
  function show(next) {
    tool = next;
    history.replaceState(history.state, '', `#${next}`);
  }

  /** The cameras lensfun knows, fetched the first time someone looks for one. */
  let cameras = $state.raw([]);
  async function loadCameras() {
    if (cameras.length) return;
    try {
      const data = await (await fetch(camerasUrl)).json();
      // Some models are listed once per mount; one entry each is enough here.
      const seen = new Map();
      for (const c of data.cameras) {
        const label = `${c.maker} ${c.model}`;
        if (c.crop > 0 && !seen.has(label)) seen.set(label, { label, crop: c.crop });
      }
      cameras = [...seen.values()];
    } catch {
      // The presets still work.
    }
  }
  function pickCamera(label) {
    v.camera = label;
    const found = cameras.find((c) => c.label === label);
    if (found) v.crop = found.crop;
  }
  const preset = $derived(SENSORS.find((s) => Math.abs(s.crop - v.crop) < 0.005)?.id ?? 'custom');

  const valid = (...xs) => xs.every((x) => Number.isFinite(x) && x > 0);
  const dof = $derived(valid(v.focal, v.aperture, v.distance, v.crop)
    ? depthOfField(v) : null);
  const exposed = $derived(valid(v.base) && Number.isFinite(v.stops) ? ndShutter(v.base, v.stops) : null);
  const stars = $derived(valid(v.starFocal, v.starAperture, v.megapixels, v.crop)
    ? starRules({ focal: v.starFocal, crop: v.crop, aperture: v.starAperture,
                  megapixels: v.megapixels }) : null);
  const placed = $derived(Number.isFinite(v.lat) && Number.isFinite(v.lon)
    && Math.abs(v.lat) <= 90 && Math.abs(v.lon) <= 180);
  const light = $derived(placed && day ? lightOfDay(new Date(`${day}T12:00`), v.lat, v.lon) : null);

  /** Metres as a photographer reads them: centimetres up close, ∞ far off. */
  function metres(v) {
    if (v === Infinity) return '∞';
    if (!Number.isFinite(v)) return '—';
    return v < 1 ? `${n(v * 100, 0)} cm` : `${n(v, v < 10 ? 2 : 1)} m`;
  }
  const clock = (date) => (date
    ? new Intl.DateTimeFormat(i18n.locale, { hour: '2-digit', minute: '2-digit' }).format(date)
    : '—');
  const span = (s) => (s ? `${clock(s.from)} – ${clock(s.to)}` : t('calc.sun.never'));

  const round3 = (v) => Math.round(v * 1000) / 1000;
  function locate() {
    if (!navigator.geolocation) {
      located = t('calc.sun.noLocation');
      return;
    }
    locating = true;
    located = '';
    navigator.geolocation.getCurrentPosition((position) => {
      v.lat = round3(position.coords.latitude);
      v.lon = round3(position.coords.longitude);
      locating = false;
      recentre = true;
    }, () => {
      located = t('calc.sun.noLocation');
      locating = false;
    }, { maximumAge: 600000, timeout: 15000 });
  }

  // -- the map ------------------------------------------------------------------
  // Leaflet over OpenStreetMap's tiles, loaded only when the golden hour is
  // open. A click puts the spot there; typed coordinates move it.
  let mapElement = $state(null);
  let map = null;
  let marker = null;
  let leaflet = null;
  /** Fly to the spot on the next update: after "use where I am", not while typing. */
  let recentre = false;

  $effect(() => {
    if (!mapElement) return;
    let gone = false;
    (async () => {
      leaflet = (await import('leaflet')).default;
      await import('leaflet/dist/leaflet.css');
      if (gone) return;
      map = leaflet.map(mapElement, { worldCopyJump: true, zoomControl: true })
        .setView(placed ? [v.lat, v.lon] : [30, 0], placed ? 9 : 2);
      leaflet.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 18,
        attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
      }).addTo(map);
      map.on('click', (e) => {
        const p = e.latlng.wrap();
        v.lat = round3(p.lat);
        v.lon = round3(p.lng);
      });
      place();
    })();
    return () => {
      gone = true;
      map?.remove();
      map = null;
      marker = null;
    };
  });

  function place() {
    if (!map || !leaflet) return;
    if (!placed) {
      marker?.remove();
      marker = null;
      return;
    }
    if (!marker) {
      marker = leaflet.circleMarker([v.lat, v.lon], {
        radius: 8, weight: 3, color: '#fff', fillColor: '#e8a33d', fillOpacity: 1,
      }).addTo(map);
    } else {
      marker.setLatLng([v.lat, v.lon]);
    }
    if (recentre) {
      map.setView([v.lat, v.lon], Math.max(map.getZoom(), 9));
      recentre = false;
    }
  }
  $effect(() => {
    // Read so the effect runs when they change.
    void v.lat;
    void v.lon;
    place();
  });
</script>

<Meta title={t('seo.calc.title')} description={t('seo.calc.description')} path="/calculators" />

<div class="shell">
  <SiteHeader tagline={t('calc.title')} />

  <main>
    <div class="page">
      <h1 class="pagetitle">{t('calc.heading')}</h1>
      <p class="lead">{t('calc.lead')}</p>

      <div class="tabs" role="tablist" aria-label={t('calc.heading')}>
        {#each TOOLS as id (id)}
          <button type="button" role="tab" id={`tab-${id}`} aria-controls="calc-panel"
                  aria-selected={tool === id} class:on={tool === id} onclick={() => show(id)}>
            {t(`calc.${id}`)}
          </button>
        {/each}
      </div>

      {#snippet sensor()}
        <fieldset class="sensor">
          <legend>{t('calc.sensor')}</legend>
          <label>
            <span>{t('calc.sensor.size')}</span>
            <select value={preset} onchange={(e) => {
              const s = SENSORS.find((x) => x.id === e.currentTarget.value);
              if (s) { v.crop = s.crop; v.camera = ''; }
            }}>
              {#each SENSORS as s (s.id)}
                <option value={s.id}>{t(`calc.sensor.${s.id}`)}</option>
              {/each}
              {#if preset === 'custom'}
                <option value="custom">{v.camera || t('calc.sensor.custom', { crop: n(v.crop, 2) })}</option>
              {/if}
            </select>
          </label>
          <label>
            <span>{t('calc.sensor.camera')}</span>
            <input list="calc-cameras" value={v.camera} placeholder={t('calc.sensor.cameraHint')}
                   onfocus={loadCameras} oninput={(e) => pickCamera(e.currentTarget.value)} />
            <datalist id="calc-cameras">
              {#each cameras as c (c.label)}<option value={c.label}></option>{/each}
            </datalist>
          </label>
        </fieldset>
      {/snippet}

      <div class="tool" id="calc-panel" role="tabpanel" aria-labelledby={`tab-${tool}`}>
        <p class="hint">{t(`calc.${tool}.hint`)}</p>

        {#if tool === 'dof'}
          <div class="body">
            <div class="inputs">
              <label><span>{t('calc.focal')}</span>
                <span class="unit"><input type="number" min="1" max="2000" step="1" bind:value={v.focal} /><em>mm</em></span></label>
              <label><span>{t('calc.aperture')}</span>
                <span class="unit"><input type="number" min="0.7" max="64" step="0.1" bind:value={v.aperture} /><em>f/</em></span></label>
              <label><span>{t('calc.distance')}</span>
                <span class="unit"><input type="number" min="0.05" max="10000" step="0.1" bind:value={v.distance} /><em>m</em></span></label>
              {@render sensor()}
            </div>
            <div class="answer">
              {#if dof}
                <span class="label">{t('calc.dof.sharp')}</span>
                <strong class="figure">{metres(dof.near)} – {metres(dof.far)}</strong>
                <span class="sub">{t('calc.dof.total', { depth: metres(dof.total) })}</span>
                <div class="aside">
                  <span class="label">{t('calc.dof.hyperfocal')}</span>
                  <strong>{metres(dof.hyperfocal)}</strong>
                  <p>{t('calc.dof.hyperNote', { half: metres(dof.hyperfocal / 2) })}</p>
                </div>
              {/if}
            </div>
          </div>
          <p class="note">{t('calc.sensor.note', { coc: n(confusion(v.crop) * 1000, 0) })}</p>

        {:else if tool === 'nd'}
          <div class="body">
            <div class="inputs">
              <label><span>{t('calc.nd.base')}</span>
                <select bind:value={v.base}>
                  {#each SHUTTERS as s (s)}<option value={s}>{duration(s)}</option>{/each}
                </select></label>
              <label><span>{t('calc.nd.stops')}</span>
                <input type="number" min="0" max="30" step="0.5" bind:value={v.stops} /></label>
              <div class="chips" role="group" aria-label={t('calc.nd.filters')}>
                {#each FILTERS as f (f.name)}
                  <button type="button" class="chip" class:on={v.stops === f.stops}
                          onclick={() => (v.stops = f.stops)}>{f.name}</button>
                {/each}
              </div>
            </div>
            <div class="answer">
              {#if exposed}
                <span class="label">{t('calc.nd.result')}</span>
                <strong class="figure">{duration(exposed)}</strong>
                {#if exposed > 30}<span class="sub">{t('calc.nd.bulb')}</span>{/if}
              {/if}
            </div>
          </div>

        {:else if tool === 'stars'}
          <div class="body">
            <div class="inputs">
              <label><span>{t('calc.focal')}</span>
                <span class="unit"><input type="number" min="1" max="2000" step="1" bind:value={v.starFocal} /><em>mm</em></span></label>
              <label><span>{t('calc.aperture')}</span>
                <span class="unit"><input type="number" min="0.7" max="64" step="0.1" bind:value={v.starAperture} /><em>f/</em></span></label>
              <label><span>{t('calc.stars.megapixels')}</span>
                <span class="unit"><input type="number" min="1" max="400" step="1" bind:value={v.megapixels} /><em>MP</em></span></label>
              {@render sensor()}
            </div>
            <div class="answer">
              {#if stars}
                <span class="label">{t('calc.stars.npf')}</span>
                <strong class="figure">{duration(stars.npf)}</strong>
                <span class="sub">{t('calc.stars.pitch', { pitch: n(stars.pitch, 1) })}</span>
                <div class="aside">
                  <span class="label">{t('calc.stars.rule500')}</span>
                  <strong>{duration(stars.rule500)}</strong>
                </div>
              {/if}
            </div>
          </div>
          <p class="note">{t('calc.stars.note')}</p>

        {:else}
          <div class="map" bind:this={mapElement}></div>
          <p class="note map-note">{t('calc.sun.mapNote')}</p>
          <div class="where">
            <label><span>{t('calc.sun.date')}</span><input type="date" bind:value={day} /></label>
            <label><span>{t('calc.sun.lat')}</span>
              <input type="number" min="-90" max="90" step="0.001" bind:value={v.lat} placeholder="40.417"
                     onchange={() => (recentre = true)} /></label>
            <label><span>{t('calc.sun.lon')}</span>
              <input type="number" min="-180" max="180" step="0.001" bind:value={v.lon} placeholder="-3.704"
                     onchange={() => (recentre = true)} /></label>
            <button type="button" class="ghost" onclick={locate} disabled={locating}>
              {locating ? t('calc.sun.locating') : t('calc.sun.here')}
            </button>
          </div>
          {#if located}<p class="note warn">{located}</p>{/if}
          {#if light}
            <div class="day">
              <div class="half">
                <h3>{t('calc.sun.morning')}</h3>
                <dl>
                  <div class="blue"><dt>{t('calc.sun.blue')}</dt><dd>{span(light.morningBlue)}</dd></div>
                  <div class="golden"><dt>{t('calc.sun.golden')}</dt><dd>{span(light.morningGolden)}</dd></div>
                  <div><dt>{t('calc.sun.sunrise')}</dt><dd>{clock(light.sunrise)}</dd></div>
                </dl>
              </div>
              <div class="half">
                <h3>{t('calc.sun.evening')}</h3>
                <dl>
                  <div><dt>{t('calc.sun.sunset')}</dt><dd>{clock(light.sunset)}</dd></div>
                  <div class="golden"><dt>{t('calc.sun.golden')}</dt><dd>{span(light.eveningGolden)}</dd></div>
                  <div class="blue"><dt>{t('calc.sun.blue')}</dt><dd>{span(light.eveningBlue)}</dd></div>
                </dl>
              </div>
            </div>
            <p class="note">{t('calc.sun.note', { zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
                                                  noon: clock(light.noon) })}</p>
          {:else}
            <p class="note">{t('calc.sun.ask')}</p>
          {/if}
        {/if}
      </div>
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../../styles/breakpoints' as *;
  @use '../../../styles/surfaces' as *;



  .page { display: grid; gap: 18px; max-width: 760px; margin: 0 auto; }
  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  /* One calculator at a time: four tabs, a row on anything wider than a
     phone, two by two on one. */
  .tabs {
    display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 4px;
    padding: 4px; border-radius: 12px; background: var(--color-panel);
    box-shadow: inset 0 0 0 1px var(--color-line);
    @include phone { grid-template-columns: 1fr 1fr; }
    button {
      min-height: 40px; padding: 8px 10px; border-radius: 9px;
      background: transparent; color: var(--color-muted); font-size: 13.5px; font-weight: 550;
      box-shadow: none;
      &:hover { color: var(--color-ink); }
      &.on { background: var(--color-panel-2); color: var(--color-accent);
             box-shadow: inset 0 0 0 1px var(--color-line); }
    }
  }

  .tool {
    @include surface;
    padding: 22px 24px;
    @include phone { padding: 16px; }
    .hint { margin: 0 0 20px; font-size: 14px; line-height: 1.55; color: var(--color-muted); }
  }

  /* What goes in on the left, what comes out on the right; one above the
     other on a phone. */
  .body {
    display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 28px;
    align-items: start;
    @include phone { grid-template-columns: 1fr; gap: 22px; }
  }

  label {
    display: grid; gap: 5px; min-width: 0;
    > span:first-child { font-size: 12.5px; color: var(--color-muted); }
  }
  input, select {
    width: 100%; min-width: 0; padding: 8px 11px; border-radius: 8px;
    border: 1px solid var(--color-line); background: var(--color-panel-2);
    color: var(--color-ink); font: inherit; font-size: 14.5px;
    font-variant-numeric: tabular-nums;
    &:focus { outline: none; border-color: var(--color-accent); }
  }
  /* A number with its unit inside the field, at the right. */
  .unit {
    position: relative; display: block;
    input { padding-right: 38px; }
    em {
      position: absolute; right: 11px; top: 50%; transform: translateY(-50%);
      font-style: normal; font-size: 12.5px; color: var(--color-muted); pointer-events: none;
    }
  }

  .inputs { display: grid; gap: 14px; }

  /* The sensor belongs to the camera, not to this calculation: set apart. */
  .sensor {
    display: grid; gap: 12px; margin: 6px 0 0; padding: 14px 0 0;
    border: 0; border-top: 1px solid var(--color-line);
    legend {
      float: left; width: 100%; margin-bottom: 2px; padding: 0;
      font-size: 11.5px; font-weight: 650; letter-spacing: .06em; text-transform: uppercase;
      color: var(--color-muted);
    }
  }

  .chips {
    display: flex; flex-wrap: wrap; gap: 6px;
    .chip {
      min-height: 0; padding: 5px 10px; border-radius: 999px; font-size: 12.5px; font-weight: 500;
      background: transparent; color: var(--color-muted);
      box-shadow: inset 0 0 0 1px var(--color-line);
      &:hover { color: var(--color-ink); }
      &.on { color: var(--color-accent); box-shadow: inset 0 0 0 1px var(--color-accent); }
    }
  }

  /* The answer: one figure, large, and whatever qualifies it under it. */
  .answer {
    display: grid; gap: 6px; align-content: start;
    padding: 18px 20px; border-radius: 12px; background: var(--color-panel-2);
    .label { font-size: 12.5px; color: var(--color-muted); }
    .figure {
      font-size: 30px; line-height: 1.15; font-weight: 650; color: var(--color-accent);
      font-variant-numeric: tabular-nums;
      @include phone { font-size: 26px; }
    }
    .sub { font-size: 13.5px; line-height: 1.5; color: var(--color-muted); }
    .aside {
      display: grid; gap: 4px; margin-top: 14px; padding-top: 14px;
      border-top: 1px solid var(--color-line);
      strong { font-size: 17px; font-variant-numeric: tabular-nums; }
      p { margin: 2px 0 0; font-size: 13px; line-height: 1.5; color: var(--color-muted); }
    }
  }

  .note {
    margin: 18px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--color-muted);
    &.warn { color: var(--color-accent); }
  }

  .map {
    /* Leaflet stacks its panes up to z-index 1000; kept inside, so the sticky
       header still passes over the map. */
    isolation: isolate;
    height: 300px; border-radius: 10px; overflow: hidden; background: var(--color-panel-2);
    @include phone { height: 240px; }
    /* Leaflet's own controls, quieter against the dark page. */
    :global(.leaflet-control-attribution) { font-size: 10px; }
  }
  .map-note { margin-top: 6px; }

  .where {
    display: grid; grid-template-columns: 1.2fr 1fr 1fr auto; gap: 12px; align-items: end;
    margin-top: 16px;
    @include phone {
      grid-template-columns: 1fr 1fr;
      label:first-child, button { grid-column: 1 / -1; }
    }
    button { min-height: 38px; }
  }

  /* The day as two halves, morning and evening, each in the order it comes. */
  .day {
    display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 22px;
    @include phone { grid-template-columns: 1fr; }
    .half { padding: 14px 16px; border-radius: 12px; background: var(--color-panel-2); }
    h3 {
      margin: 0 0 8px; font-size: 11.5px; font-weight: 650; letter-spacing: .06em;
      text-transform: uppercase; color: var(--color-muted);
    }
    dl { display: grid; gap: 8px; margin: 0; }
    dl div { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
    dt { font-size: 13.5px; color: var(--color-muted); }
    dd { margin: 0; font-size: 14.5px; font-weight: 600; font-variant-numeric: tabular-nums; }
    .golden dt { color: #e0a43a; }
    .blue dt { color: #79a6e6; }
  }
</style>
