<script>
  /** The photographer's arithmetic, worked out as the numbers are typed:
   *  depth of field, ND filters, sharp stars, and the golden and blue hours. */
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { t, n, localized, i18n } from '$lib/i18n.svelte.js';
  import { NAME } from '$lib/brand.js';
  import { app } from '$lib/state.svelte.js';
  import {
    SENSORS, FILTERS, SHUTTERS, confusion, depthOfField, duration, lightOfDay, ndShutter, starRules,
  } from '$lib/calc.js';
  import camerasUrl from '$lib/wasm/lenses/cameras.json?url';

  /** What was typed, remembered in this browser between visits. */
  const STORE = 'lumiraw.calc';
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(STORE)) ?? {};
    } catch {
      return {};
    }
  })();
  const start = {
    crop: 1, camera: '', focal: 35, aperture: 8, distance: 3, base: 1 / 60, stops: 10,
    starFocal: 20, starAperture: 2.8, megapixels: 24, lat: null, lon: null, ...saved,
  };
  let crop = $state(start.crop);
  let camera = $state(start.camera);
  let focal = $state(start.focal);
  let aperture = $state(start.aperture);
  let distance = $state(start.distance);
  let base = $state(start.base);
  let stops = $state(start.stops);
  let starFocal = $state(start.starFocal);
  let starAperture = $state(start.starAperture);
  let megapixels = $state(start.megapixels);
  let lat = $state(start.lat);
  let lon = $state(start.lon);
  let day = $state(new Date().toISOString().slice(0, 10));
  let locating = $state(false);
  let located = $state('');

  $effect(() => {
    const all = { crop, camera, focal, aperture, distance, base, stops, starFocal, starAperture,
                  megapixels, lat, lon };
    try {
      localStorage.setItem(STORE, JSON.stringify(all));
    } catch {
      // Private window or storage refused: nothing is remembered, that is all.
    }
  });

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
    camera = label;
    const found = cameras.find((c) => c.label === label);
    if (found) crop = found.crop;
  }
  const preset = $derived(SENSORS.find((s) => Math.abs(s.crop - crop) < 0.005)?.id ?? 'custom');

  const valid = (...v) => v.every((x) => Number.isFinite(x) && x > 0);
  const dof = $derived(valid(focal, aperture, distance, crop)
    ? depthOfField({ focal, aperture, distance, crop }) : null);
  const exposed = $derived(valid(base) && Number.isFinite(stops) ? ndShutter(base, stops) : null);
  const stars = $derived(valid(starFocal, starAperture, megapixels, crop)
    ? starRules({ focal: starFocal, crop, aperture: starAperture, megapixels }) : null);
  const light = $derived(Number.isFinite(lat) && Number.isFinite(lon) && day
    ? lightOfDay(new Date(`${day}T12:00`), lat, lon) : null);

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

  function locate() {
    if (!navigator.geolocation) {
      located = t('calc.sun.noLocation');
      return;
    }
    locating = true;
    located = '';
    navigator.geolocation.getCurrentPosition((position) => {
      lat = Math.round(position.coords.latitude * 1000) / 1000;
      lon = Math.round(position.coords.longitude * 1000) / 1000;
      locating = false;
    }, () => {
      located = t('calc.sun.noLocation');
      locating = false;
    }, { maximumAge: 600000, timeout: 15000 });
  }
</script>

<Meta title={t('seo.calc.title')} description={t('seo.calc.description')} path="/calculators" />

<div class="shell">
  <header>
    <a class="brand" href={localized('/')}>
      {NAME}
      <span>
        <span class="tagline">{t('calc.title')}</span>
        {#if app.version}<span class="ver">v{app.version}</span>{/if}
      </span>
    </a>
    <a class="back" href={localized('/')}>{t('calc.back')}</a>
    <LocalePicker />
  </header>

  <main>
    <div class="mx-auto grid max-w-[980px] gap-4">
      <h1 class="pagetitle">{t('calc.heading')}</h1>
      <p class="lead">{t('calc.lead')}</p>

      <section class="panel sensor">
        <h2>{t('calc.sensor')}</h2>
        <div class="fields">
          <label>
            <span>{t('calc.sensor.size')}</span>
            <select value={preset} onchange={(e) => {
              const s = SENSORS.find((x) => x.id === e.currentTarget.value);
              if (s) { crop = s.crop; camera = ''; }
            }}>
              {#each SENSORS as s (s.id)}
                <option value={s.id}>{t(`calc.sensor.${s.id}`)}</option>
              {/each}
              {#if preset === 'custom'}<option value="custom">{t('calc.sensor.custom', { crop: n(crop, 2) })}</option>{/if}
            </select>
          </label>
          <label class="grow">
            <span>{t('calc.sensor.camera')}</span>
            <input list="calc-cameras" value={camera} placeholder={t('calc.sensor.cameraHint')}
                   onfocus={loadCameras} oninput={(e) => pickCamera(e.currentTarget.value)} />
            <datalist id="calc-cameras">
              {#each cameras as c (c.label)}<option value={c.label}></option>{/each}
            </datalist>
          </label>
          <label class="narrow">
            <span>{t('calc.sensor.crop')}</span>
            <input type="number" min="0.3" max="10" step="0.01" bind:value={crop}
                   oninput={() => (camera = '')} />
          </label>
        </div>
        <small>{t('calc.sensor.note', { coc: n(confusion(crop) * 1000, 0) })}</small>
      </section>

      <div class="grid2">
        <section class="panel">
          <h2>{t('calc.dof')}</h2>
          <p class="hint">{t('calc.dof.hint')}</p>
          <div class="fields">
            <label><span>{t('calc.focal')}</span>
              <input type="number" min="1" max="2000" step="1" bind:value={focal} /><em>mm</em></label>
            <label><span>{t('calc.aperture')}</span>
              <input type="number" min="0.7" max="64" step="0.1" bind:value={aperture} /><em>f/</em></label>
            <label><span>{t('calc.distance')}</span>
              <input type="number" min="0.05" max="10000" step="0.1" bind:value={distance} /><em>m</em></label>
          </div>
          {#if dof}
            <dl class="answers">
              <div><dt>{t('calc.dof.near')}</dt><dd>{metres(dof.near)}</dd></div>
              <div><dt>{t('calc.dof.far')}</dt><dd>{metres(dof.far)}</dd></div>
              <div><dt>{t('calc.dof.total')}</dt><dd>{metres(dof.total)}</dd></div>
              <div class="key"><dt>{t('calc.dof.hyperfocal')}</dt><dd>{metres(dof.hyperfocal)}</dd></div>
            </dl>
            <p class="note">{t('calc.dof.hyperNote', { half: metres(dof.hyperfocal / 2) })}</p>
          {/if}
        </section>

        <section class="panel">
          <h2>{t('calc.nd')}</h2>
          <p class="hint">{t('calc.nd.hint')}</p>
          <div class="fields">
            <label><span>{t('calc.nd.base')}</span>
              <select bind:value={base}>
                {#each SHUTTERS as s (s)}<option value={s}>{duration(s)}</option>{/each}
              </select></label>
            <label><span>{t('calc.nd.stops')}</span>
              <input type="number" min="0" max="30" step="0.5" bind:value={stops} /></label>
          </div>
          <div class="chips">
            {#each FILTERS as f (f.name)}
              <button type="button" class="chip" class:on={stops === f.stops}
                      onclick={() => (stops = f.stops)}>{f.name}</button>
            {/each}
          </div>
          {#if exposed}
            <dl class="answers">
              <div class="key"><dt>{t('calc.nd.result')}</dt><dd>{duration(exposed)}</dd></div>
            </dl>
            {#if exposed > 30}<p class="note">{t('calc.nd.bulb')}</p>{/if}
          {/if}
        </section>

        <section class="panel">
          <h2>{t('calc.stars')}</h2>
          <p class="hint">{t('calc.stars.hint')}</p>
          <div class="fields">
            <label><span>{t('calc.focal')}</span>
              <input type="number" min="1" max="2000" step="1" bind:value={starFocal} /><em>mm</em></label>
            <label><span>{t('calc.aperture')}</span>
              <input type="number" min="0.7" max="64" step="0.1" bind:value={starAperture} /><em>f/</em></label>
            <label><span>{t('calc.stars.megapixels')}</span>
              <input type="number" min="1" max="400" step="1" bind:value={megapixels} /><em>MP</em></label>
          </div>
          {#if stars}
            <dl class="answers">
              <div class="key"><dt>{t('calc.stars.npf')}</dt><dd>{duration(stars.npf)}</dd></div>
              <div><dt>{t('calc.stars.rule500')}</dt><dd>{duration(stars.rule500)}</dd></div>
              <div><dt>{t('calc.stars.pitch')}</dt><dd>{n(stars.pitch, 1)} µm</dd></div>
            </dl>
            <p class="note">{t('calc.stars.note')}</p>
          {/if}
        </section>

        <section class="panel">
          <h2>{t('calc.sun')}</h2>
          <p class="hint">{t('calc.sun.hint')}</p>
          <div class="fields">
            <label><span>{t('calc.sun.date')}</span><input type="date" bind:value={day} /></label>
            <label><span>{t('calc.sun.lat')}</span>
              <input type="number" min="-90" max="90" step="0.001" bind:value={lat} placeholder="40.417" /></label>
            <label><span>{t('calc.sun.lon')}</span>
              <input type="number" min="-180" max="180" step="0.001" bind:value={lon} placeholder="-3.704" /></label>
          </div>
          <button type="button" class="ghost small" onclick={locate} disabled={locating}>
            {locating ? t('calc.sun.locating') : t('calc.sun.here')}
          </button>
          {#if located}<p class="note warn">{located}</p>{/if}
          {#if light}
            <dl class="answers sun">
              <div class="blue"><dt>{t('calc.sun.morningBlue')}</dt><dd>{span(light.morningBlue)}</dd></div>
              <div class="golden"><dt>{t('calc.sun.morningGolden')}</dt><dd>{span(light.morningGolden)}</dd></div>
              <div><dt>{t('calc.sun.sunrise')}</dt><dd>{clock(light.sunrise)}</dd></div>
              <div><dt>{t('calc.sun.noon')}</dt><dd>{clock(light.noon)}</dd></div>
              <div><dt>{t('calc.sun.sunset')}</dt><dd>{clock(light.sunset)}</dd></div>
              <div class="golden"><dt>{t('calc.sun.eveningGolden')}</dt><dd>{span(light.eveningGolden)}</dd></div>
              <div class="blue"><dt>{t('calc.sun.eveningBlue')}</dt><dd>{span(light.eveningBlue)}</dd></div>
            </dl>
            <p class="note">{t('calc.sun.note', { zone: Intl.DateTimeFormat().resolvedOptions().timeZone })}</p>
          {:else}
            <p class="note">{t('calc.sun.ask')}</p>
          {/if}
        </section>
      </div>
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../../styles/breakpoints' as *;
  @use '../../../styles/surfaces' as *;

  /* The header as on the merge page: the name is the way home. */
  a.brand {
    color: inherit; text-decoration: none;
    &:hover {
      color: var(--color-accent);
      .tagline, .ver { color: var(--color-muted); }
    }
  }

  header {
    .back {
      margin-left: auto; color: var(--color-muted); text-decoration: none; font-size: 13px;
      &:hover { color: var(--color-ink); }
      @include phone {
        order: 3; width: 100%; margin-left: 0; padding: 10px 0 4px; font-size: 14px;
      }
    }
    :global(.picker) { margin-left: 0; @include phone { margin-left: auto; } }
  }

  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  .grid2 {
    display: grid; gap: 16px; grid-template-columns: repeat(2, minmax(0, 1fr));
    @include narrow { grid-template-columns: 1fr; }
  }

  .panel {
    @include surface;
    padding: 16px 18px; min-width: 0;
    h2 { margin: 0 0 4px; font-size: 15px; font-weight: 650; }
    .hint { margin: 0 0 12px; font-size: 13px; line-height: 1.5; color: var(--color-muted); }
    > small { display: block; margin-top: 8px; font-size: 12.5px; color: var(--color-muted); }
  }

  .fields {
    display: grid; gap: 10px; grid-template-columns: repeat(3, minmax(0, 1fr));
    margin-bottom: 12px;
    @include phone { grid-template-columns: 1fr 1fr; }
    label {
      position: relative; display: grid; gap: 4px; min-width: 0;
      > span { font-size: 12px; color: var(--color-muted); }
      /* The unit, inside the field at its right. */
      em {
        position: absolute; right: 10px; bottom: 9px; font-style: normal;
        font-size: 12.5px; color: var(--color-muted); pointer-events: none;
      }
    }
    input, select {
      width: 100%; min-width: 0; padding: 7px 10px; border-radius: 8px;
      border: 1px solid var(--color-line); background: var(--color-panel-2);
      color: var(--color-ink); font: inherit; font-size: 14px;
      font-variant-numeric: tabular-nums;
      &:focus { outline: none; border-color: var(--color-accent); }
    }
    input[type='number'] { padding-right: 34px; }
  }
  /* The sensor row: the size, the camera (the most room), the factor. */
  .sensor .fields {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr) minmax(0, 0.6fr);
    margin-bottom: 0;
    @include phone {
      grid-template-columns: 1fr 1fr;
      .grow { grid-column: 1 / -1; grid-row: 2; }
    }
  }

  .chips {
    display: flex; flex-wrap: wrap; gap: 6px; margin: -2px 0 12px;
    .chip {
      min-height: 0; padding: 4px 9px; border-radius: 999px; font-size: 12px; font-weight: 500;
      background: transparent; color: var(--color-muted);
      box-shadow: inset 0 0 0 1px var(--color-line);
      &.on { color: var(--color-accent); box-shadow: inset 0 0 0 1px var(--color-accent); }
    }
  }

  .answers {
    display: grid; gap: 2px; margin: 0;
    div {
      display: flex; justify-content: space-between; align-items: baseline; gap: 12px;
      padding: 7px 10px; border-radius: 8px; font-size: 13.5px;
      &:nth-child(odd) { background: var(--color-panel-2); }
    }
    dt { color: var(--color-muted); }
    dd { margin: 0; font-variant-numeric: tabular-nums; font-weight: 600; text-align: right; }
    /* The figure the panel is for. */
    .key dd { color: var(--color-accent); font-size: 15px; }
    &.sun {
      margin-top: 12px;
      .golden dt { color: #e0a43a; }
      .blue dt { color: #6f9fe0; }
    }
  }

  .note {
    margin: 10px 2px 0; font-size: 12.5px; line-height: 1.5; color: var(--color-muted);
    &.warn { color: var(--color-accent); }
  }

  button.small { padding: 5px 12px; min-height: 0; font-size: 12.5px; }
</style>
