<script>
  /** The photographer's arithmetic, one calculator at a time, worked out as
   *  the numbers are typed, for two pages: the calculators (depth of field,
   *  ND filters) and the sky (the golden hour and the night over a spot,
   *  dark skies, sharp stars). `tools` are the page's, in order; `page` its
   *  name in the strings (`calc` or `sky`) and `path` its address. What is
   *  typed is shared between them. */
  import { untrack } from 'svelte';
  import SiteHeader from '$lib/components/SiteHeader.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import SunView from '$lib/components/SunView.svelte';
  import SavedSpots from '$lib/components/SavedSpots.svelte';
  import StarPreview from '$lib/components/StarPreview.svelte';
  import DarkSky from '$lib/components/DarkSky.svelte';
  import { i18n, t, n, clock } from '$lib/i18n.svelte.js';
  import { RAD, atMinute, isPlace, localDay, round3, spotKey } from '$lib/geo.js';
  import { openMap, spotMarker } from '$lib/map.js';
  import { readLink, writeLink } from '$lib/spotlink.js';
  import { skyOfNight } from '$lib/night.js';
  import { skyOrUnlit } from '$lib/darksky.js';
  import {
    EXPOSURES, SENSORS, FILTERS, SHUTTERS, cameraName, confusion, depthOfField, duration, fieldOfView, lightOfDay,
    ndShutter, nearestStep, starRules, starTrail,
  } from '$lib/calc.js';
  import camerasUrl from '$lib/wasm/lenses/cameras.json?url';

  let { tools, page, path } = $props();
  let asked = $state(null);
  const tool = $derived(asked ?? tools[0]);

  /** What was typed, remembered in this browser between visits. */
  const STORE = 'lumiraw.calc';
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(STORE)) ?? {};
    } catch {
      return {};
    }
  })();
  /** A spot, day and time the address asks for, over what was remembered. */
  const link = (() => {
    try {
      return readLink(location.hash);
    } catch {
      return {};
    }
  })();
  /** Every input, in one object, so remembering them is one line. */
  const v = $state({
    crop: 1, camera: '', focal: 35, aperture: 8, distance: 3, base: 1 / 60, stops: 10,
    starFocal: 20, starAperture: 2.8, megapixels: 24, lat: null, lon: null,
    frame: false, portrait: false, tilt: 20, spots: [], ...saved,
    ...(link.lat === undefined ? {} : { lat: link.lat, lon: link.lon }),
  });
  let day = $state(link.day ?? localDay(new Date()));
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
    const hash = readLink(location.hash).tool;
    if (tools.includes(hash)) asked = hash;
  });
  function show(next) {
    asked = next;
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
        const label = cameraName(c.maker, c.model);
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
  // The stars' preview: the exposure on the slider's steps, opening on what
  // NPF allows for each lens and sensor; the rules marked on the same steps,
  // so each rule's button reads as that rule.
  const marks = $derived(stars ? { npf: nearestStep(EXPOSURES, stars.npf), rule500: nearestStep(EXPOSURES, stars.rule500) } : null);
  let starTime = $state(0);
  let declination = $state(0);
  $effect(() => {
    if (marks) starTime = marks.npf;
  });
  const trail = $derived(stars ? starTrail({ focal: v.starFocal, seconds: EXPOSURES[starTime], pitch: stars.pitch, declination }) : 0);
  const verdict = $derived(!marks ? null : starTime <= marks.npf ? 'sharp' : starTime <= marks.rule500 ? 'soft' : 'trails');
  /** Where a step falls along the slider. */
  const onSlider = (step) => `${(step / (EXPOSURES.length - 1)) * 100}%`;
  const placed = $derived(isPlace(v.lat, v.lon));
  const light = $derived(placed && day ? lightOfDay(new Date(`${day}T12:00`), v.lat, v.lon) : null);

  // The golden hour's settings in the address, a moment after they
  // stop changing: browsers limit how often it can be written.
  const here = $derived(light ? writeLink({ lat: v.lat, lon: v.lon, day, ...look }) : '');
  $effect(() => {
    if (tool !== 'sun' || !here) return;
    const hash = here;
    const wait = setTimeout(() => history.replaceState(history.state, '', hash), 250);
    return () => clearTimeout(wait);
  });
  /** A link pasted over this page's address changes only the hash. Taken as
   *  it is, not by reloading: Firefox would put the old values back in the inputs. */
  function followed() {
    const next = readLink(location.hash);
    if (next.lat === undefined) return;
    if (tools.includes(next.tool)) asked = next.tool;
    Object.assign(look, viewOf(next));
    day = next.day;
    recentre = true;
    v.lat = next.lat;
    v.lon = next.lon;
  }

  // -- the stars' sky -----------------------------------------------------------
  // The spot's night, for the Sharp stars preview: at the time picked on the
  // golden hour if it is dark then, or else at the darkest of that night.
  let atlas = $state.raw(null);
  $effect(() => {
    atlas = null;
    if (tool !== 'stars' || !placed) return;
    let stale = false;
    skyOrUnlit(v.lat, v.lon).then((found) => stale || (atlas = found));
    return () => (stale = true);
  });
  const starSky = $derived(atlas && day ? skyOfNight({ lat: v.lat, lon: v.lon, day, minutes: look.minutes, ratio: atlas.ratio }) : null);
  const starClock = $derived.by(() => {
    if (!starSky) return '';
    const date = atMinute(day, starSky.minutes);
    return `${date.toLocaleDateString(i18n.locale, { day: 'numeric', month: 'short' })}, ${clock(date)}`;
  });

  /** Metres as a photographer reads them: centimetres up close, ∞ far off. */
  function metres(v) {
    if (v === Infinity) return '∞';
    if (!Number.isFinite(v)) return '—';
    return v < 1 ? `${n(v * 100, 0)} cm` : `${n(v, v < 10 ? 2 : 1)} m`;
  }
  const span = (s) => (s ? `${clock(s.from)} – ${clock(s.to)}` : t('calc.sun.never'));

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
  // Loaded only when the golden hour is open. A click puts the spot there;
  // typed coordinates move it.
  let mapElement = $state(null);
  let map = null;
  let marker = null;
  let leaflet = null;
  /** Fly to the spot on the next update: after "use where I am", not while typing. */
  let recentre = false;
  /** The settings of the view under the map (SunView), kept here so they outlast
   *  its tab; which way it looks and how wide it is are drawn on the map as a wedge. */
  const viewOf = ({ lat, lon, day: on, minutes, facing, around, dome }) => ({
    minutes, facing, around, dome, opened: spotKey(lat, lon, on),
  });
  let look = $state(link.lat === undefined
    ? { minutes: 18 * 60, facing: 270, around: false, dome: false, opened: null }
    : viewOf(link));
  let shown = $state(100);
  /** What the lens takes in, if it is marked on the view. */
  const field = $derived(valid(v.starFocal, v.crop) ? fieldOfView({ focal: v.starFocal, crop: v.crop }) : null);
  const frame = $derived(v.frame && field ? {
    focal: v.starFocal, tilt: v.tilt,
    width: v.portrait ? field.height : field.width,
    height: v.portrait ? field.width : field.height,
  } : null);
  /** How wide the wedge on the map is: the lens, if marked, or the view. */
  const across = $derived(frame?.width ?? shown);
  let wedge = null;
  let relabel = null;

  $effect(() => {
    if (!mapElement) return;
    let gone = false;
    // Opened once for each element: where it opens, read but not followed.
    const start = untrack(() => (placed ? { centre: [v.lat, v.lon], zoom: 9 } : { centre: [30, 0], zoom: 2 }));
    openMap(mapElement, {
      ...start,
      onpick: (lat, lon) => {
        // Picked from far out: closer, to see where it landed.
        recentre = map.getZoom() < 6;
        v.lat = lat;
        v.lon = lon;
      },
      bigger: () => ({ more: t('calc.map.bigger'), less: t('calc.map.smaller') }),
    }).then((opened) => {
      if (gone) return opened.map.remove();
      ({ leaflet, map, relabel } = opened);
      // Under the spot's dot, which sits in the overlay pane.
      map.createPane('wedge').style.zIndex = 350;
      place();
    });
    return () => {
      gone = true;
      map?.remove();
      map = marker = wedge = relabel = null;
    };
  });
  $effect(() => {
    void i18n.locale;
    relabel?.();
  });

  function place() {
    if (!map || !leaflet) return;
    marker?.remove();
    marker = placed ? spotMarker(leaflet, [v.lat, v.lon]).addTo(map) : null;
    if (placed && recentre) {
      map.setView([v.lat, v.lon], Math.max(map.getZoom(), 9));
      recentre = false;
    }
    aim();
  }

  /** The wedge from the spot the way the view looks: a marker, so the same
   *  size on the screen at any zoom, drawn as a sector of a circle. */
  function aim() {
    if (!map || !leaflet) return;
    if (!placed || !light) {
      wedge?.remove();
      wedge = null;
      return;
    }
    const R = 70;
    const point = (bearing) => `${R + R * Math.sin(bearing * RAD)},${R - R * Math.cos(bearing * RAD)}`;
    const from = look.facing - across / 2;
    const to = look.facing + across / 2;
    const shape = across >= 360
      ? `<circle cx="${R}" cy="${R}" r="${R}"/>`
      : `<path d="M${R},${R} L${point(from)} A${R},${R} 0 ${across > 180 ? 1 : 0} 1 ${point(to)} Z"/>`;
    const icon = leaflet.divIcon({
      className: 'wedge', iconSize: [2 * R, 2 * R],
      html: `<svg viewBox="0 0 ${2 * R} ${2 * R}" fill="rgba(232,163,61,.25)" stroke="#e8a33d" stroke-width="1.5">${shape}</svg>`,
    });
    if (wedge) wedge.setLatLng([v.lat, v.lon]).setIcon(icon);
    else wedge = leaflet.marker([v.lat, v.lon], { icon, pane: 'wedge', interactive: false, keyboard: false }).addTo(map);
  }
  $effect(() => {
    // Read so the effect runs when they change.
    void v.lat;
    void v.lon;
    place();
  });
  $effect(() => {
    void look.facing;
    void across;
    void light;
    aim();
  });
</script>

<svelte:window onhashchange={followed} />

<Meta title={t(`seo.${page}.title`)} description={t(`seo.${page}.description`)} {path} />

<div class="shell">
  <SiteHeader tagline={t(`${page}.title`)} />

  <main>
    <div class="page">
      <h1 class="pagetitle">{t(`${page}.heading`)}</h1>
      <p class="lead">{t(`${page}.lead`)}</p>

      <div class="tabs" role="tablist" aria-label={t(`${page}.heading`)} style:--tabs={tools.length}>
        {#each tools as id (id)}
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
          {#if stars}
            <StarPreview aperture={v.starAperture} pitch={stars.pitch} {trail} seconds={EXPOSURES[starTime]} focal={v.starFocal}
                         limit={starSky?.limit} glow={starSky?.glow} />
            <p class="note">
              {#if starSky}
                {t('calc.stars.sky', { when: starClock, mag: n(starSky.limit, 1) })}
                {atlas.zone ? t('calc.sun.night.zone', { zone: atlas.zone }) : ''}
              {:else if !placed}
                {t('calc.stars.noSpot')}
              {/if}
            </p>
            <div class="exposure">
              <label>
                <span>{t('calc.stars.preview.exposure')} <strong>{duration(EXPOSURES[starTime])}</strong></span>
                <span class="slider">
                  <input type="range" min="0" max={EXPOSURES.length - 1} step="1" bind:value={starTime} />
                  <i class="npf" style:left={onSlider(marks.npf)}></i>
                  <i class="r500" style:left={onSlider(marks.rule500)}></i>
                </span>
              </label>
              <div class="chips">
                <button type="button" class="chip npf" class:on={starTime === marks.npf}
                        onclick={() => (starTime = marks.npf)}>{t('calc.stars.npf')}</button>
                <button type="button" class="chip r500" class:on={starTime === marks.rule500}
                        onclick={() => (starTime = marks.rule500)}>{t('calc.stars.rule500')}</button>
              </div>
              <label>
                <span>{t('calc.stars.preview.where')}</span>
                <select bind:value={declination}>
                  <option value={0}>{t('calc.stars.preview.equator')}</option>
                  <option value={45}>{t('calc.stars.preview.mid')}</option>
                  <option value={80}>{t('calc.stars.preview.pole')}</option>
                </select>
              </label>
            </div>
            <p class="verdict {verdict}">{t(`calc.stars.preview.${verdict}`, { pixels: n(trail, 1) })}</p>
          {/if}
          <p class="note">{t('calc.stars.note')}</p>

        {:else if tool === 'dark'}
          <DarkSky bind:lat={v.lat} bind:lon={v.lon} {locate} {locating}
                   onpick={(lat, lon) => { v.lat = lat; v.lon = lon; recentre = true; show('sun'); }} />
          {#if located}<p class="note warn">{located}</p>{/if}

        {:else}
          <div class="where">
            <label><span>{t('calc.sun.date')}</span><input type="date" bind:value={day} /></label>
            <label><span>{t('calc.sun.lat')}</span>
              <input type="number" min="-90" max="90" step="0.001" bind:value={v.lat} placeholder="40.417"
                     onchange={() => { recentre = true; place(); }} /></label>
            <label><span>{t('calc.sun.lon')}</span>
              <input type="number" min="-180" max="180" step="0.001" bind:value={v.lon} placeholder="-3.704"
                     onchange={() => { recentre = true; place(); }} /></label>
            <button type="button" class="ghost" onclick={locate} disabled={locating}>
              {locating ? t('calc.sun.locating') : t('calc.sun.here')}
            </button>
          </div>
          {#if located}<p class="note warn">{located}</p>{/if}
          <SavedSpots bind:spots={v.spots} lat={v.lat} lon={v.lon} {placed}
                      onpick={(spot) => { recentre = true; v.lat = spot.lat; v.lon = spot.lon; }} />
          <p class="note map-note">{t('calc.sun.mapNote')}</p>
          <div class="map" bind:this={mapElement}></div>
          {#if light}
            <div class="lens">
              <label class="switch"><input type="checkbox" bind:checked={v.frame} /> {t('calc.sun.frame')}</label>
              {#if v.frame}
                <label><span>{t('calc.focal')}</span>
                  <span class="unit"><input type="number" min="1" max="2000" step="1" bind:value={v.starFocal} /><em>mm</em></span></label>
                <label class="switch"><input type="checkbox" bind:checked={v.portrait} /> {t('calc.sun.frame.portrait')}</label>
                <label class="tilt"><span>{t('calc.sun.frame.tilt')} <strong>{v.tilt}°</strong></span>
                  <input type="range" min="-10" max="80" step="1" bind:value={v.tilt} /></label>
                {#if frame}<p class="note">{t('calc.sun.frame.size', { width: n(frame.width, 0), height: n(frame.height, 0), crop: n(v.crop, 2) })}</p>{/if}
              {/if}
            </div>
            <SunView lat={v.lat} lon={v.lon} bind:day {light} link={here} {frame} bind:opened={look.opened} bind:minutes={look.minutes}
                     bind:facing={look.facing} bind:around={look.around} bind:dome={look.dome} bind:shown>
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
            </SunView>
          {:else}
            <p class="note">{t('calc.sun.ask')}</p>
          {/if}
        {/if}
      </div>
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;
  @use '../../styles/surfaces' as *;



  .page { display: grid; gap: 18px; max-width: 760px; margin: 0 auto; }
  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  /* One calculator at a time: the tabs in a row on anything wider than a
     phone, two by two on one, an odd last one across. */
  .tabs {
    display: grid; grid-template-columns: repeat(var(--tabs), minmax(0, 1fr)); gap: 4px;
    padding: 4px; border-radius: 12px; background: var(--color-panel);
    box-shadow: inset 0 0 0 1px var(--color-line);
    @include phone { grid-template-columns: 1fr 1fr; button:nth-child(odd):last-child { grid-column: 1 / -1; } }
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
  input:not([type='range']), select {
    width: 100%; min-width: 0; padding: 8px 11px; border-radius: 8px;
    border: 1px solid var(--color-line); background-color: var(--color-panel-2);
    color: var(--color-ink); font: inherit; font-size: 14.5px;
    font-variant-numeric: tabular-nums;
    &:focus { outline: none; border-color: var(--color-accent); }
  }
  select { padding-right: 32px; }
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

  /* The stars' preview: the exposure and where the camera points, under the patch of sky. */
  .exposure {
    display: grid; grid-template-columns: minmax(0, 1fr) auto 210px; gap: 12px 16px; align-items: end;
    margin-top: 14px;
    @include phone { grid-template-columns: 1fr; }
    strong { color: var(--color-ink); font-variant-numeric: tabular-nums; }
    .chips { padding-bottom: 2px; }
  }
  /* NPF and the 500 rule marked where they fall on the slider. */
  .slider {
    position: relative; display: block; padding-bottom: 10px;
    i {
      position: absolute; bottom: 0; width: 2px; height: 8px; margin-left: -1px; border-radius: 1px;
      &.npf { background: var(--color-accent); }
      &.r500 { background: var(--color-muted); }
    }
  }
  .verdict {
    margin: 12px 0 0; font-size: 13.5px; line-height: 1.5;
    &.soft { color: #e0a43a; }
    &.trails { color: #e0705a; }
  }

  .map {
    /* Leaflet stacks its panes up to z-index 1000; kept inside, so the sticky
       header still passes over the map. */
    isolation: isolate;
    height: 420px; border-radius: 10px; overflow: hidden; background: var(--color-panel-2);
    @include phone { height: 240px; }
    /* Leaflet's own controls, quieter against the dark page. */
    :global(.leaflet-control-attribution) { font-size: 10px; }
    /* The way the view under it looks (aim()). */
    :global(.wedge svg) { display: block; width: 100%; height: 100%; }
  }
  .map-note { margin: 10px 0 8px; }
  .lens {
    display: flex; flex-wrap: wrap; align-items: end; gap: 10px 16px; margin-top: 12px; font-size: 13px;
    .switch { display: flex; align-items: center; gap: 6px; min-height: 36px; cursor: pointer; }
    .switch input { width: auto; margin: 0; accent-color: var(--color-accent); }
    .unit { width: 110px; }
    .tilt { flex: 1 1 160px; }
    .note { flex-basis: 100%; margin: 0; }
  }

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
    margin-bottom: 16px;
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
