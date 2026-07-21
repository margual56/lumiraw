<script>
  /** Exposure merge: several frames of one scene in, one wider negative out. */
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import Progress from '$lib/components/Progress.svelte';
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import * as api from '$lib/api.js';
  import { t, n, signed, shutter, loose } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { app, resetForNewPhoto, FRAME } from '$lib/state.svelte.js';

  /** One entry per frame the module is holding, in the order it holds them. */
  let frames = $state([]);
  let align = $state(true);
  let deghost = $state(0.5);
  let result = $state(null);
  let error = $state('');
  let over = $state(false);
  let input;
  // The same file can legitimately be added twice, so the list cannot be keyed
  // by anything that came out of the file.
  let uid = 0;

  api.setProgressListener((fraction, code) => {
    app.progress = code === 'done' ? null : { fraction, code };
  });

  $effect(() => {
    api.ready().catch(() => {});
    // Arriving here with frames left over from a previous visit would merge
    // pictures the page is not showing.
    api.mergeReset().catch(() => {});
    return () => { api.mergeReset().catch(() => {}); };
  });

  // The frames are sorted by how much light they caught, which is the order
  // anyone thinks of a bracket in, and the middle one is the reference the
  // merge puts the others on.
  const ordered = $derived([...frames].sort((a, b) => a.exposure - b.exposure));
  const reference = $derived(ordered.length ? ordered[Math.floor(ordered.length / 2)] : null);
  const range = $derived(ordered.length > 1
    ? Math.log2(ordered[ordered.length - 1].exposure / ordered[0].exposure)
    : 0);

  const stopsOf = (frame) =>
    (reference && frame.exposure > 0 && reference.exposure > 0)
      ? Math.log2(frame.exposure / reference.exposure) : 0;

  async function add(files) {
    over = false;
    error = '';
    if (result) {
      // The last merge is finished and its frames are gone; anything dropped
      // now is the start of a new bracket, not an addition to that one.
      result = null;
      await api.mergeReset().catch(() => {});
    }
    for (const file of Array.from(files ?? [])) {
      app.busy = true;
      app.busyKey = 'busy.mergeAdd';
      app.busyParams = { file: file.name };
      try {
        const info = await api.mergeAdd(file);
        frames = [...frames, { ...info, file: file.name, uid: ++uid }];
      } catch (err) {
        error = errorText(err);
      } finally {
        app.busy = false;
        app.busyKey = '';
        app.progress = null;
      }
    }
    if (input) input.value = '';
  }

  async function remove(index) {
    error = '';
    result = null;
    await api.mergeRemove(index).catch(() => {});
    frames = frames.filter((_, i) => i !== index);
  }

  async function clear() {
    error = '';
    result = null;
    await api.mergeReset().catch(() => {});
    frames = [];
  }

  async function run() {
    error = '';
    result = null;
    app.busy = true;
    app.busyKey = 'busy.merge';
    app.busyParams = {};
    try {
      const info = await api.mergeFinish({ align, deghost });
      // The module hands its frames to the merge and keeps nothing, so the list
      // has to go with them.
      result = { ...info, referenceName: frames[info.merge?.reference]?.name ?? info.file };
      frames = [];
    } catch (err) {
      error = errorText(err);
    } finally {
      app.busy = false;
      app.busyKey = '';
      app.progress = null;
    }
  }

  /** The merged frame is already the module's current picture, so developing
   *  it is a matter of pointing the wizard at it. */
  function develop() {
    const info = result;
    resetForNewPhoto();
    app.id = info.id;
    app.info = info;
    app.step = FRAME;
    app.message = '';
    goto(`${base}/`);
  }

  const percent = (value) => `${n(value * 100, value < 0.01 ? 2 : 1)} %`;
  const maxShift = $derived(result?.merge
    ? Math.max(0, ...result.merge.shifts.flatMap(([x, y]) => [Math.abs(x), Math.abs(y)]))
    : 0);
</script>

<div class="shell">
  <header>
    <div class="brand">
      autoraw
      <span>
        <span class="tagline">{t('merge.title')}</span>
        {#if app.version}<span class="ver">v{app.version}</span>{/if}
      </span>
    </div>
    <a class="back" href="{base}/">{t('merge.back')}</a>
    <LocalePicker />
  </header>
  <Progress />

  <main>
    <div class="column">
      <p class="lead">{t('merge.lead')}</p>

      <details class="how">
        <summary>{t('merge.how.title')}</summary>
        <p>{t('merge.how.body')}</p>
      </details>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <label
        class="drop"
        class:compact={frames.length > 0}
        class:over
        ondragenter={(e) => { e.preventDefault(); over = true; }}
        ondragover={(e) => { e.preventDefault(); over = true; }}
        ondragleave={() => (over = false)}
        ondrop={(e) => { e.preventDefault(); add(e.dataTransfer.files); }}
      >
        <input
          bind:this={input}
          type="file"
          multiple
          accept=".arw,.sr2,.srf,.cr2,.cr3,.nef,.raf,.orf,.rw2,.pef,.dng"
          onchange={(e) => add(e.currentTarget.files)}
        />
        <strong>{frames.length ? t('merge.dropMore') : t('merge.drop')}</strong>
        <span>{t('merge.formats')}</span>
      </label>

      {#if error}<p class="error">{error}</p>{/if}

      {#if frames.length}
        <section class="panel">
          <div class="panel-head">
            <h2>{t('merge.frames')}</h2>
            <button class="ghost small" onclick={clear}>{t('merge.clear')}</button>
          </div>
          <ul class="frames">
            {#each ordered as frame (frame.uid)}
              <li class:is-reference={frame === reference}>
                <span class="stops">{signed(stopsOf(frame), 1)} EV</span>
                <span class="name">
                  {frame.name}
                  {#if frame === reference}<em>{t('merge.reference')}</em>{/if}
                </span>
                <span class="shot">
                  {t('merge.exposureOf', {
                    shutter: shutter(frame.shutter_s),
                    aperture: loose(frame.aperture), iso: frame.iso,
                  })}
                </span>
                <button
                  class="ghost small"
                  onclick={() => remove(frames.indexOf(frame))}
                >{t('merge.remove')}</button>
              </li>
            {/each}
          </ul>
          {#if frames.length > 1}
            <p class="range">{t('merge.range', { stops: n(range, 1) })}</p>
          {:else}
            <p class="range">{t('merge.needTwo')}</p>
          {/if}
        </section>

        <section class="panel">
          <h2>{t('merge.options')}</h2>
          <label class="option">
            <input type="checkbox" bind:checked={align} />
            <span>
              <strong>{t('merge.align')}</strong>
              <small>{t('merge.alignHint')}</small>
            </span>
          </label>
          <div class="option slider">
            <div class="row">
              <strong>{t('merge.deghost')}</strong>
              <span class="value">
                {deghost === 0 ? t('merge.deghostOff') : `${n(deghost * 100, 0)} %`}
              </span>
            </div>
            <input type="range" min="0" max="1" step="0.05" bind:value={deghost} />
            <small>{t('merge.deghostHint')}</small>
          </div>
          <button
            class="run"
            disabled={frames.length < 2 || app.busy}
            onclick={run}
          >{app.busy ? t('merge.running') : t('merge.run')}</button>
        </section>
      {/if}

      {#if result?.merge}
        <section class="panel done">
          <h2>{t('merge.result')}</h2>
          <p>{t('merge.resultBody', {
            frames: result.merge.frames,
            range: n(result.merge.range_stops, 1),
            reference: result.referenceName,
          })}</p>
          <ul class="notes">
            <li>{maxShift > 0
              ? t('merge.shifts', { pixels: maxShift })
              : t('merge.noShifts')}</li>
            {#if result.merge.ghosted > 0}
              <li>{t('merge.ghosted', { percent: percent(result.merge.ghosted) })}</li>
            {/if}
            {#if result.merge.uncovered > 0.0005}
              <li class="warn">{t('merge.uncovered',
                { percent: percent(result.merge.uncovered) })}</li>
            {/if}
          </ul>
          <div class="actions">
            <button onclick={develop}>{t('merge.develop')}</button>
            <button class="ghost" onclick={clear}>{t('merge.again')}</button>
          </div>
        </section>
      {/if}
    </div>
  </main>
</div>

<style>
  .ver { color: var(--line); font-variant-numeric: tabular-nums; }
  .tagline + .ver { margin-left: 6px; }
  /* The way back and the language sit together at the right, so the header
     reads as a name on one side and controls on the other. */
  header .back {
    margin-left: auto; color: var(--muted); text-decoration: none; font-size: 13px;
  }
  header :global(.picker) { margin-left: 0; }
  header .back:hover { color: var(--ink); }

  .column { max-width: 880px; margin-inline: auto; display: grid; gap: 16px; }
  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--muted); }

  .how { font-size: 13.5px; color: var(--muted); }
  .how summary { cursor: pointer; color: var(--accent); }
  .how p { margin: 8px 0 0; line-height: 1.6; }

  .drop {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; min-height: 200px; cursor: pointer;
    border: 1.5px dashed var(--line); border-radius: 16px; background: var(--panel);
    transition: border-color .15s, background .15s;
  }
  .drop.compact { min-height: 110px; }
  .drop:hover, .drop.over { border-color: var(--accent); background: var(--panel-2); }
  .drop input { display: none; }
  .drop strong { font-size: 18px; font-weight: 600; }
  .drop span { color: var(--muted); font-size: 13px; text-align: center; padding: 0 12px; }

  .error {
    margin: 0; padding: 10px 12px; border-radius: 8px;
    border: 1px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--ink); font-size: 13.5px;
  }

  .panel {
    padding: 16px 18px; border: 1px solid var(--line); border-radius: var(--radius);
    background: var(--panel);
  }
  .panel h2 { margin: 0 0 10px; font-size: 15px; font-weight: 650; }
  .panel-head { display: flex; align-items: center; justify-content: space-between; }
  .panel-head h2 { margin: 0 0 10px; }

  .frames { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .frames li {
    display: grid; grid-template-columns: 76px 1fr auto auto;
    align-items: center; gap: 12px;
    padding: 8px 10px; border-radius: 8px; font-size: 13.5px;
  }
  .frames li:nth-child(odd) { background: var(--panel-2); }
  .frames li.is-reference { box-shadow: inset 2px 0 0 var(--accent); }
  .stops {
    font-variant-numeric: tabular-nums; font-weight: 600;
    color: var(--ink); text-align: right;
  }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name em {
    margin-left: 6px; font-style: normal; font-size: 11px; text-transform: uppercase;
    letter-spacing: .04em; color: var(--accent);
  }
  .shot { color: var(--muted); font-size: 12.5px; font-variant-numeric: tabular-nums; }
  button.small { padding: 4px 10px; min-height: 0; font-size: 12px; }

  .range { margin: 10px 2px 0; font-size: 13px; color: var(--muted); }

  .option { display: flex; gap: 10px; align-items: flex-start; margin-bottom: 14px; }
  .option span { display: grid; gap: 2px; }
  .option strong { font-size: 14px; font-weight: 600; }
  .option small { color: var(--muted); font-size: 12.5px; line-height: 1.5; }
  .option input[type='checkbox'] { margin-top: 3px; width: 16px; height: 16px; accent-color: var(--accent); }
  .slider { display: block; }
  .slider .row { display: flex; justify-content: space-between; align-items: baseline; }
  .slider .value { color: var(--muted); font-size: 12.5px; font-variant-numeric: tabular-nums; }
  .slider input[type='range'] { width: 100%; margin: 6px 0 2px; accent-color: var(--accent); }
  .slider small { display: block; color: var(--muted); font-size: 12.5px; line-height: 1.5; }
  .run { margin-top: 4px; }

  .done { border-color: var(--ok); }
  .done h2 { color: var(--ok); }
  .done p { margin: 0 0 8px; font-size: 14px; }
  .notes { margin: 0 0 14px; padding-left: 18px; color: var(--muted); font-size: 13px; }
  .notes li { margin: 4px 0; line-height: 1.5; }
  .notes .warn { color: var(--ink); }
  .actions { display: flex; gap: 10px; flex-wrap: wrap; }

  @media (max-width: 640px) {
    header .back { font-size: 12px; }
    .frames li {
      grid-template-columns: 62px 1fr auto;
      grid-template-areas: 'stops name remove' 'stops shot remove';
      row-gap: 2px;
    }
    .stops { grid-area: stops; }
    .name { grid-area: name; }
    .shot { grid-area: shot; }
    .frames li button { grid-area: remove; }
  }
</style>
