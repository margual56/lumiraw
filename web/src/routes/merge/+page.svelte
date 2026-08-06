<script>
  /** Exposure merge: several frames of one scene in, one wider negative out. */
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import Progress from '$lib/components/Progress.svelte';
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import * as api from '$lib/api.js';
  import { t, n, signed, shutter, loose } from '$lib/i18n.svelte.js';
  import { errorText, RAW_ACCEPT } from '$lib/format.js';
  import { NAME } from '$lib/brand.js';
  import { app, resetForNewPhoto, FRAME } from '$lib/state.svelte.js';

  /** One entry per frame the module is holding, in the order it holds them. */
  let frames = $state([]);
  let align = $state(true);
  // Automatic by default: the merge can measure how much of the scene actually
  // moved, and a fixed setting cannot. The slider is still here to overrule it.
  let deghostAuto = $state(true);
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

  /** What the module makes of the bracket so far. */
  let findings = $state([]);

  async function check() {
    try {
      const out = await api.mergeCheck();
      findings = out.findings ?? [];
    } catch {
      findings = [];
    }
  }

  // Which frames a finding is about, named rather than numbered.
  const named = (finding) =>
    finding.frames.map((i) => frames[i]?.name).filter(Boolean).join(', ');

  /** The brackets the module found in what was dropped, if it found more than one. */
  const brackets = $derived(findings.filter((f) => f.code === 'separate_shot')
    .map((f) => f.frames));
  const split = $derived(brackets.length > 1);

  // The split has a panel of its own, with an action beside each bracket, so
  // it does not belong in the list of sentences underneath as well.
  const remarks = $derived(findings.filter((f) => f.code !== 'separate_shot'));

  // How far apart a bracket's own frames are, which is what decides whether
  // it is worth merging at all.
  const spanOf = (members) => {
    const light = members.map((i) => frames[i]?.exposure).filter((e) => e > 0);
    return light.length > 1 ? Math.log2(Math.max(...light) / Math.min(...light)) : 0;
  };

  // Both frames of a pair are named in the finding, since the pair is what is
  // wrong.
  const dialIgnored = $derived(new Set(
    findings.filter((f) => f.code === 'dial_ignored')
      .flatMap((f) => f.frames)
      .filter((i) => Math.abs(frames[i]?.exposure_comp ?? 0) > 0.05)));

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
        await check();
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
    await check();
  }

  /** Keep one of the brackets that were found, and merge it. */
  async function mergeOnly(members) {
    error = '';
    const keep = new Set(members);
    for (let i = frames.length - 1; i >= 0; i -= 1) {
      if (keep.has(i)) continue;
      await api.mergeRemove(i).catch(() => {});
      frames = frames.filter((_, j) => j !== i);
    }
    await check();
    await run();
  }

  async function clear() {
    error = '';
    result = null;
    await api.mergeReset().catch(() => {});
    frames = [];
    findings = [];
  }

  async function run() {
    error = '';
    result = null;
    app.busy = true;
    app.busyKey = 'busy.merge';
    app.busyParams = {};
    try {
      const info = await api.mergeFinish({ align, deghost: deghostAuto ? -1 : deghost });
      // The module hands its frames to the merge and keeps nothing, so the list
      // has to go with them.
      result = {
        ...info,
        referenceName: frames[info.merge?.reference]?.name ?? info.file,
        findings: (info.merge?.findings ?? []).map((f) => ({ ...f, names: named(f) })),
      };
      frames = [];
      findings = [];
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
    // A merged frame is not one of the files in a roll.
    app.roll = [];
    app.rollAt = -1;
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
    <a class="brand" href="{base}/">
      {NAME}
      <span>
        <span class="tagline">{t('merge.title')}</span>
        {#if app.version}<span class="ver">v{app.version}</span>{/if}
      </span>
    </a>
    <a class="back" href="{base}/">{t('merge.back')}</a>
    <LocalePicker />
  </header>
  <Progress />

  <main>
    <div class="mx-auto grid max-w-[880px] gap-4">
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
          accept={RAW_ACCEPT}
          onchange={(e) => add(e.currentTarget.files)}
        />
        <strong>{frames.length ? t('merge.dropMore') : t('merge.drop')}</strong>
        <span>{t('merge.formats')}</span>
      </label>

      {#if error}<p class="error">{error}</p>{/if}

      {#if frames.length}
        {#if split}
          <!--
            First, above the list of frames, because until this is settled the list
            underneath is a list of unrelated photographs and everything it says about
            "this bracket" is about a bracket that does not exist.
          -->
          <section class="panel split">
            <h2>{t('merge.split.title', { count: brackets.length })}</h2>
            <p>{t('merge.split.body')}</p>
            <ul class="brackets">
              {#each brackets as members, nth (nth)}
                <li>
                  <span class="which">{nth + 1}</span>
                  <span class="what">
                    <strong>{members.map((i) => frames[i]?.name).join(', ')}</strong>
                    <small>
                      {members.length > 1
                        ? t('merge.split.frames', { count: members.length,
                                                    range: n(spanOf(members), 1) })
                        : t('merge.split.single')}
                    </small>
                  </span>
                  {#if members.length > 1}
                    <button
                      class="small"
                      disabled={app.busy}
                      onclick={() => mergeOnly(members)}
                    >{t('merge.split.merge', { count: members.length })}</button>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section class="panel">
          <div class="flex items-center justify-between">
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
                  {#if dialIgnored.has(frames.indexOf(frame))}
                    <em class="warn" title={t('merge.unhonouredWhy')}>
                      {t('merge.dialled', { ev: signed(frame.exposure_comp ?? 0, 1) })}
                    </em>
                  {/if}
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
          <!-- The range is the distance between the darkest and brightest frame of a bracket. -->
          {#if split}
            <p class="range">{t('merge.split.pick')}</p>
          {:else if frames.length > 1}
            <p class="range">{t('merge.range', { stops: n(range, 1) })}</p>
          {:else}
            <p class="range">{t('merge.needTwo')}</p>
          {/if}
          {#if remarks.length}
            <ul class="findings">
              {#each remarks as finding (finding.code + finding.frames)}
                <li>{t(`merge.found.${finding.code}`,
                       { frames: named(finding), value: n(Math.abs(finding.value), 1),
                         count: finding.frames.length })}</li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="panel">
          <h2>{t('merge.options')}</h2>
          <label class="option flex items-start gap-2.5">
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
                {#if deghostAuto}
                  {result?.merge?.deghost_used == null
                    ? t('merge.deghostAuto')
                    : t('merge.deghostChose',
                        { value: n(result.merge.deghost_used * 100, 0) })}
                {:else}
                  {deghost === 0 ? t('merge.deghostOff') : `${n(deghost * 100, 0)} %`}
                {/if}
              </span>
            </div>
            <input
              type="range" min="0" max="1" step="0.05"
              bind:value={deghost}
              oninput={() => (deghostAuto = false)}
            />
            <label class="auto">
              <input type="checkbox" bind:checked={deghostAuto} />
              <span>{t('merge.deghostAuto')}</span>
            </label>
            <small>{t('merge.deghostHint')}</small>
          </div>
          <!--
            Still offered when the frames fall into several brackets, since refusing
            outright would be deciding for someone who may have meant it.
          -->
          <button
            class="run"
            class:secondary={split}
            disabled={frames.length < 2 || app.busy}
            onclick={run}
          >{app.busy
              ? t('merge.running')
              : split
                ? t('merge.split.anyway', { count: frames.length })
                : t('merge.run')}</button>
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
            {#each result.findings as finding (finding.code + finding.frames)}
              <li class="warn">{t(`merge.found.${finding.code}`,
                                  { frames: finding.names, value: n(Math.abs(finding.value), 1),
                                    count: finding.frames.length })}</li>
            {/each}
            {#if result.merge.uncovered > 0.0005}
              <li class="warn">{t('merge.uncovered',
                { percent: percent(result.merge.uncovered) })}</li>
            {/if}
          </ul>
          <div class="flex flex-wrap gap-2.5">
            <button onclick={develop}>{t('merge.develop')}</button>
            <button class="ghost" onclick={clear}>{t('merge.again')}</button>
          </div>
        </section>
      {/if}
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;
  @use '../../styles/surfaces' as *;

  /* The name is the way home, which is where anyone looks for it. */
  a.brand {
    color: inherit; text-decoration: none;
    &:hover {
      color: var(--color-accent);
      .tagline, .ver { color: var(--color-muted); }
    }
  }

  header {
    /* The way back and the language sit together at the right, so the header
       reads as a name on one side and controls on the other. */
    .back {
      margin-left: auto; color: var(--color-muted); text-decoration: none; font-size: 13px;
      &:hover { color: var(--color-ink); }
      @include card { font-size: 12px; }
    }
    :global(.picker) { margin-left: 0; }
  }

  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  .how {
    font-size: 13.5px; color: var(--color-muted);
    summary { cursor: pointer; color: var(--color-accent); }
    p { margin: 8px 0 0; line-height: 1.6; }
  }

  /* A shorter target than the upload step's, and shorter again once there are
     frames listed under it.  The rest of `.drop` is in app.scss. */
  .drop {
    min-height: 200px;
    &.compact { min-height: 110px; }
    strong { font-size: 18px; }
    span { text-align: center; padding: 0 12px; }
  }

  .error {
    margin: 0; padding: 10px 12px; border-radius: 8px;
    border: 1px solid var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
    color: var(--color-ink); font-size: 13.5px;
  }

  .panel {
    @include surface;
    padding: 16px 18px;
    h2 { margin: 0 0 10px; font-size: 15px; font-weight: 650; }
    /* Merged: the same panel, in the colour that says it worked. */
    &.done {
      border-color: var(--color-ok);
      h2 { color: var(--color-ok); }
      p { margin: 0 0 8px; font-size: 14px; }
    }
  }

  .frames {
    list-style: none; margin: 0; padding: 0; display: grid; gap: 2px;
    li {
      display: grid; grid-template-columns: 76px 1fr auto auto;
      align-items: center; gap: 12px;
      padding: 8px 10px; border-radius: 8px; font-size: 13.5px;
      &:nth-child(odd) { background: var(--color-panel-2); }
      &.is-reference { box-shadow: inset 2px 0 0 var(--color-accent); }
      /* Too narrow for four columns: the exposure drops under the filename
         and the two of them share the middle. */
      @include card {
        grid-template-columns: 62px 1fr auto;
        grid-template-areas: 'stops name remove' 'stops shot remove';
        row-gap: 2px;
        button { grid-area: remove; }
      }
    }
  }
  .stops {
    font-variant-numeric: tabular-nums; font-weight: 600;
    color: var(--color-ink); text-align: right;
    @include card { grid-area: stops; }
  }
  .name {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    @include card { grid-area: name; }
    em {
      margin-left: 6px; font-style: normal; font-size: 11px; text-transform: uppercase;
      letter-spacing: .04em; color: var(--color-accent);
      /* The dial setting, shown only on a frame that did not get what it asked for. */
      &.warn { text-transform: none; font-size: 11.5px; cursor: help; }
    }
  }
  .shot {
    color: var(--color-muted); font-size: 12.5px; font-variant-numeric: tabular-nums;
    @include card { grid-area: shot; }
  }

  button.small { padding: 4px 10px; min-height: 0; font-size: 12px; }
  /* Everything the module found odd, under the frames it is about. */
  .findings {
    margin: 8px 0 0; padding-left: 18px;
    font-size: 13px; line-height: 1.5; color: var(--color-accent);
    li { margin: 4px 0; }
  }

  .range {
    margin: 10px 2px 0; font-size: 13px; color: var(--color-muted);
  }


  .option {
    margin-bottom: 14px;
    span { display: grid; gap: 2px; }
    strong { font-size: 14px; font-weight: 600; }
    small { color: var(--color-muted); font-size: 12.5px; line-height: 1.5; }
    input[type='checkbox'] {
      margin-top: 3px; width: 16px; height: 16px; accent-color: var(--color-accent);
    }
  }
  .slider {
    .row { display: flex; justify-content: space-between; align-items: baseline; }
    .value {
      color: var(--color-muted); font-size: 12.5px; font-variant-numeric: tabular-nums;
    }
    input[type='range'] { width: 100%; margin: 6px 0 2px; accent-color: var(--color-accent); }
    .auto {
      display: flex; align-items: center; gap: 6px; margin: 2px 0 6px;
      color: var(--color-muted); font-size: 12.5px;
      input[type='checkbox'] { margin-top: 0; width: 14px; height: 14px; }
      span { display: inline; }
    }
    small { display: block; }
  }
  .run { margin-top: 4px; }

  /* Merging everything is still allowed when the frames fall into several
     brackets, but it is no longer the thing the eye lands on. */
  .run.secondary {
    background: transparent; color: var(--color-muted);
    box-shadow: inset 0 0 0 1px var(--color-line);
  }

  .split {
    h2 { color: var(--color-accent); }
    p { margin: 0 0 12px; font-size: 13.5px; line-height: 1.55; color: var(--color-muted); }
  }

  .brackets {
    list-style: none; margin: 0; padding: 0; display: grid; gap: 2px;
    li {
      display: grid; grid-template-columns: 28px 1fr auto;
      align-items: center; gap: 12px;
      padding: 8px 10px; border-radius: 8px; font-size: 13.5px;
      &:nth-child(odd) { background: var(--color-panel-2); }
    }
    /* Which bracket this is, in the order they were shot. */
    .which {
      display: grid; place-items: center;
      width: 22px; height: 22px; border-radius: 999px;
      background: var(--color-panel-2); color: var(--color-muted);
      font-size: 12px; font-variant-numeric: tabular-nums;
    }
    .what { display: grid; gap: 2px; min-width: 0; }
    .what strong { overflow-wrap: anywhere; font-weight: 600; }
    .what small { color: var(--color-muted); font-size: 12.5px; }
    /* Narrow: the action drops below the frames it acts on. */
    @include card {
      li { grid-template-columns: 22px 1fr; }
      button { grid-column: 2; justify-self: start; }
    }
  }

  .notes {
    margin: 0 0 14px; padding-left: 18px; color: var(--color-muted); font-size: 13px;
    li { margin: 4px 0; line-height: 1.5; }
    /* The one note that is a warning rather than a remark. */
    .warn { color: var(--color-ink); }
  }
</style>
