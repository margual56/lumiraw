<script>
  /** Culling: a whole shoot in, every photo scored by NIMA, the best of each
   *  moment picked and handed to the developer as a roll. */
  import { goto } from '$app/navigation';
  import Progress from '$lib/components/Progress.svelte';
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import * as api from '$lib/api.js';
  import * as critic from '$lib/critic.js';
  import {
    DEFAULTS, blend, blinking, exposurePenalty, groupBursts, likeness, pick, relativeSharpness, toCsv,
    CLIPPED,
  } from '$lib/cull.js';
  import { t, n, localized } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { NAME } from '$lib/brand.js';
  import { app } from '$lib/state.svelte.js';
  import { filesFromDrop, pickMany } from '$lib/roll.svelte.js';

  /**
   * One entry per file, in the order they were dropped: { uid, file, name,
   * raw, thumb, captured, aesthetic, technical, feature, failed }. Replaced
   * rather than mutated, since the features are too big to watch deeply.
   */
  let items = $state.raw([]);
  /** What the ranking was last worked out from: items, at most a few times a
   *  second while scoring, since grouping compares every pair of neighbours. */
  let shown = $state.raw([]);

  /** Every lever, remembered in this browser between visits. */
  const STORE = 'lumiraw.cull';
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(STORE)) ?? {};
    } catch {
      return {};
    }
  })();
  const initial = { ...DEFAULTS, keepMode: 'percent', keepValue: 10, bursts: true, ...saved };
  let weight = $state(initial.weight);
  let focus = $state(initial.focus);
  let faults = $state(initial.faults);
  let eyes = $state(initial.eyes);
  let variety = $state(initial.variety);
  let gap = $state(initial.gap);
  let keepMode = $state(initial.keepMode);
  let keepValue = $state(initial.keepValue);
  let bursts = $state(initial.bursts);
  let similarity = $state(initial.similarity);
  /** The levers past the first three, folded away until asked for. */
  let refining = $state(false);

  const levers = $derived({ weight, focus, faults, eyes, variety });
  $effect(() => {
    const all = { weight, focus, faults, eyes, variety, gap, similarity, keepMode, keepValue, bursts };
    try {
      localStorage.setItem(STORE, JSON.stringify(all));
    } catch {
      // Private window or storage refused: the levers just are not remembered.
    }
  });

  function defaults() {
    ({ weight, focus, faults, eyes, variety, gap, similarity } = DEFAULTS);
    keepMode = 'percent';
    keepValue = 10;
    bursts = true;
  }
  /** Picks changed by hand: uid -> picked. */
  let overrides = $state.raw(new Map());
  /** Groups opened to show every frame, by their leader's uid. */
  let open = $state.raw(new Set());

  let running = $state(false);
  let stopping = false;
  let loading = $state(null);
  let done = $state(0);
  let total = $state(0);
  let started = 0;
  let error = $state('');
  let skipped = $state(0);
  let over = $state(false);
  let input;
  let uid = 0;

  $effect(() => {
    api.ready().catch(() => {});
    return () => {
      stopping = true;
      for (const it of items) if (it.thumb) URL.revokeObjectURL(it.thumb);
    };
  });

  let timer = null;
  function refresh() {
    clearTimeout(timer);
    timer = null;
    shown = items.filter((it) => it.feature);
  }
  function refreshSoon() {
    if (!timer) timer = setTimeout(refresh, 400);
  }

  /** Dot products between pairs of photos, kept while they last: they do
   *  not change as the gallery grows or the levers move. */
  const pairs = new WeakMap();
  const like = $derived(likeness(shown, pairs));
  const groups = $derived(bursts
    ? groupBursts(shown, { gap, similarity, like })
    : shown.map((it) => [it]));
  const ranking = $derived(pick(groups, levers, { mode: keepMode, value: keepValue },
                                shown.length, like));
  const sharp = $derived(relativeSharpness(shown));
  const isPicked = (it) => (overrides.has(it.uid) ? overrides.get(it.uid) : ranking.picked.has(it));
  /** Every group, its best frame first: the picks in the order they were
   *  made, then the rest, best first. */
  const ordered = $derived(ranking.groups);
  const pickedCount = $derived(shown.filter(isPicked).length);
  /** Picks the developer cannot take. */
  const notRaw = $derived(shown.filter((it) => isPicked(it) && !it.raw).length);
  const keepMax = $derived(keepMode === 'percent' ? 100 : Math.max(1, shown.length));
  /** On a phone, whether the controls are unfolded. */
  let tuning = $state(false);
  const failed = $derived(items.filter((it) => it.failed));

  const eta = $derived.by(() => {
    if (!running || done < 2) return '';
    const left = ((performance.now() - started) / 1000 / done) * (total - done);
    return left < 90 ? `${Math.ceil(left)} s` : `${Math.ceil(left / 60)} min`;
  });

  async function add(list) {
    over = false;
    if (running) return;
    error = '';
    const all = [...(list ?? [])];
    const usable = all.filter((f) => critic.isRaw(f.name) || critic.isImage(f.name))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    skipped = all.length - usable.length;
    if (input) input.value = '';
    if (!usable.length) {
      if (all.length) error = t('cull.none');
      return;
    }
    const start = items.length;
    items = [...items, ...usable.map((file) => ({
      uid: ++uid, file, name: file.name, raw: critic.isRaw(file.name),
    }))];
    running = true;
    stopping = false;
    try {
      app.busy = true;
      app.busyKey = 'busy.cullLoad';
      app.busyParams = {};
      await critic.ready((fraction) => { loading = fraction; });
      loading = null;
      done = 0;
      total = usable.length;
      started = performance.now();
      await critic.scoreAll(usable, {
        stopped: () => stopping,
        oneach: (i, out, err) => {
          const k = start + i;
          items = items.with(k, out ? { ...items[k], ...out }
                                    : { ...items[k], failed: errorText(err) });
          done += 1;
          app.busyKey = 'busy.cull';
          app.busyParams = { done, total };
          refreshSoon();
        },
      });
    } catch (err) {
      error = errorText(err);
    } finally {
      running = false;
      loading = null;
      app.busy = false;
      app.busyKey = '';
      refresh();
    }
  }

  function clear() {
    for (const it of items) if (it.thumb) URL.revokeObjectURL(it.thumb);
    items = [];
    shown = [];
    overrides = new Map();
    open = new Set();
    error = '';
    skipped = 0;
  }

  function toggle(it) {
    const next = new Map(overrides);
    next.set(it.uid, !isPicked(it));
    overrides = next;
  }

  function unfold(leader) {
    const next = new Set(open);
    if (next.has(leader.uid)) next.delete(leader.uid);
    else next.add(leader.uid);
    open = next;
  }

  function csv() {
    const picked = new Set(shown.filter(isPicked));
    const blob = new Blob([toCsv(ranking, picked, levers, sharp)], { type: 'text/csv' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${NAME.toLowerCase()}-ranking.csv`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  /** The picked raws, as a roll in the developer. */
  async function develop() {
    error = '';
    const files = shown.filter((it) => isPicked(it) && it.raw).map((it) => it.file);
    if (!files.length) {
      error = t('cull.noRaws');
      return;
    }
    await pickMany(files);
    if (app.id) goto(localized('/'));
    else error = app.message;
  }

  const mark = (v) => n(v, 1);
  /** How a photo's mark was made up, for its badge's tooltip. */
  function why(it) {
    const m = ranking.marks.get(it);
    if (!m) return '';
    const parts = [t('cull.why.blend', { value: mark(blend(it, weight)) })];
    const exposure = exposurePenalty(it, faults);
    if (exposure >= 0.05) parts.push(t('cull.why.exposure', { value: mark(exposure) }));
    if (m.eyes >= 0.05) parts.push(t('cull.why.eyes', { value: mark(m.eyes) }));
    if (m.focus >= 0.05) parts.push(t('cull.why.focus', { value: mark(m.focus) }));
    return parts.join(' · ');
  }
  /** A lever's strength in words: off, or a percentage. */
  const strength = (v) => (v <= 0 ? t('cull.off') : `${n(v * 100, 0)} %`);
  /** A share as a percentage, with a decimal while it is small. */
  const share = (v) => `${n((v ?? 0) * 100, v < 0.1 ? 1 : 0)} %`;
</script>

<Meta title={t('seo.cull.title')} description={t('seo.cull.description')} path="/cull" />

<div class="shell">
  <header>
    <a class="brand" href={localized('/')}>
      {NAME}
      <span>
        <span class="tagline">{t('cull.title')}</span>
        {#if app.version}<span class="ver">v{app.version}</span>{/if}
      </span>
    </a>
    <a class="back" href={localized('/')}>{t('cull.back')}</a>
    <LocalePicker />
  </header>
  <Progress />

  <main>
    <div class="mx-auto grid max-w-[1100px] gap-4">
      <div class="intro grid gap-4">
        <h1 class="pagetitle">{t('cull.heading')}</h1>
        <p class="lead">{t('cull.lead')}</p>
        <details class="how">
          <summary>{t('cull.how.title')}</summary>
          <p>{t('cull.how.body')}</p>
        </details>
      </div>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <label
        class="drop"
        class:compact={items.length > 0}
        class:over
        class:disabled={running}
        ondragenter={(e) => { e.preventDefault(); over = !running; }}
        ondragover={(e) => { e.preventDefault(); over = !running; }}
        ondragleave={() => (over = false)}
        ondrop={async (e) => { e.preventDefault(); add(await filesFromDrop(e.dataTransfer)); }}
      >
        <input
          bind:this={input}
          type="file"
          multiple
          disabled={running}
          accept={critic.ACCEPT}
          onchange={(e) => add(e.currentTarget.files)}
        />
        <strong>
          <span class="on-hover">{items.length ? t('cull.dropMore') : t('cull.drop')}</span>
          <span class="on-touch">{items.length ? t('cull.dropMore.touch') : t('cull.drop.touch')}</span>
        </strong>
        <span>{t('cull.formats')}</span>
      </label>

      {#if error}<p class="error">{error}</p>{/if}
      {#if skipped > 0}<p class="note">{t('cull.skipped', { count: skipped })}</p>{/if}
      {#if failed.length}
        <p class="note">{t('cull.failed', { names: failed.map((it) => it.name).join(', ') })}</p>
      {/if}

      {#if running}
        <section class="panel status">
          {#if loading !== null}
            <p>{t('cull.loading', { percent: n(loading * 100, 0) })}</p>
            <div class="meter"><div style:width={`${(loading * 100).toFixed(1)}%`}></div></div>
          {:else}
            <p>
              {t('cull.scoring', { done, total })}{eta ? ` · ${t('cull.eta', { time: eta })}` : ''}
            </p>
            <div class="meter"><div style:width={`${total ? (done / total) * 100 : 0}%`}></div></div>
          {/if}
          <button class="ghost small" onclick={() => (stopping = true)}>{t('cull.stop')}</button>
        </section>
      {/if}

      {#if shown.length}
        <!--
          Everything that changes the picks, kept in view over the photos it
          changes. On a phone only the tally and the actions stay; the
          controls fold away behind "Adjust".
        -->
        <section class="toolbar" class:tuning>
          <div class="head">
            <p class="tally">
              <strong>{pickedCount}</strong>
              <span>{t('cull.tally', { total: shown.length, groups: groups.length })}</span>
            </p>
            <button class="ghost adjust" aria-expanded={tuning} onclick={() => (tuning = !tuning)}>
              {t('cull.adjust')}
            </button>
            <div class="actions">
              {#if !running}
                <button class="ghost" onclick={clear}>{t('cull.again')}</button>
              {/if}
              <button class="ghost" onclick={csv} title={t('cull.csv')}>CSV</button>
              <button class="develop" onclick={develop} disabled={running || app.busy}
                      title={t('cull.developHint')}>{t('cull.develop')}</button>
            </div>
          </div>

          <div class="tune">
            <div class="ctl">
              <span class="label" id="cull-weight">{t('cull.weight')}</span>
              <div class="slide">
                <span class="end">{t('cull.weight.technical')}</span>
                <input type="range" min="0" max="1" step="0.05" bind:value={weight}
                       aria-labelledby="cull-weight" />
                <span class="end">{t('cull.weight.aesthetic')}</span>
              </div>
            </div>

            <div class="ctl">
              <span class="label" id="cull-keep">{t('cull.keep')}</span>
              <div class="keep" role="group" aria-labelledby="cull-keep">
                <div class="stepper">
                  <button type="button" aria-label={t('cull.keep.less')}
                          onclick={() => (keepValue = Math.max(1, keepValue - 1))}>−</button>
                  <input type="number" min="1" max={keepMax} bind:value={keepValue}
                         aria-labelledby="cull-keep" />
                  <button type="button" aria-label={t('cull.keep.more')}
                          onclick={() => (keepValue = Math.min(keepMax, keepValue + 1))}>+</button>
                </div>
                <div class="seg" role="radiogroup" aria-labelledby="cull-keep">
                  <button type="button" role="radio" aria-checked={keepMode === 'count'}
                          onclick={() => (keepMode = 'count')}>{t('cull.keep.count')}</button>
                  <button type="button" role="radio" aria-checked={keepMode === 'percent'}
                          onclick={() => (keepMode = 'percent')}>%</button>
                </div>
              </div>
            </div>

            <div class="ctl">
              <label class="switch" title={t('cull.burstsHint')}>
                <input type="checkbox" role="switch" bind:checked={bursts} />
                <span class="label">{t('cull.bursts')}</span>
              </label>
              <div class="slide" class:off={!bursts}>
                <span class="end">{t('cull.similarity.loose')}</span>
                <input type="range" min="0.6" max="0.95" step="0.01" disabled={!bursts}
                       bind:value={similarity} aria-label={t('cull.similarity')} />
                <span class="end">{t('cull.similarity.strict')}</span>
              </div>
            </div>
          </div>

          <div class="more">
            <button type="button" class="linkish" aria-expanded={refining}
                    onclick={() => (refining = !refining)}>
              {t('cull.moreLevers')}<span class="caret" aria-hidden="true">▾</span>
            </button>
            {#if refining}
              <button type="button" class="linkish" onclick={defaults}>{t('cull.defaults')}</button>
            {/if}
          </div>

          {#if refining}
            <div class="tune refine">
              <div class="ctl">
                <div class="row">
                  <span class="label" id="cull-focus">{t('cull.focus')}</span>
                  <output>{strength(focus)}</output>
                </div>
                <input type="range" min="0" max="1" step="0.05" bind:value={focus}
                       aria-labelledby="cull-focus" />
                <small>{t('cull.focusHint')}</small>
              </div>
              <div class="ctl">
                <div class="row">
                  <span class="label" id="cull-faults">{t('cull.faults')}</span>
                  <output>{strength(faults)}</output>
                </div>
                <input type="range" min="0" max="1" step="0.05" bind:value={faults}
                       aria-labelledby="cull-faults" />
                <small>{t('cull.faultsHint')}</small>
              </div>
              <div class="ctl">
                <div class="row">
                  <span class="label" id="cull-eyes">{t('cull.eyes')}</span>
                  <output>{strength(eyes)}</output>
                </div>
                <input type="range" min="0" max="1" step="0.05" bind:value={eyes}
                       aria-labelledby="cull-eyes" />
                <small>{t('cull.eyesHint')}</small>
              </div>
              <div class="ctl">
                <div class="row">
                  <span class="label" id="cull-variety">{t('cull.variety')}</span>
                  <output>{strength(variety)}</output>
                </div>
                <input type="range" min="0" max="1" step="0.05" bind:value={variety}
                       aria-labelledby="cull-variety" />
                <small>{t('cull.varietyHint')}</small>
              </div>
              <div class="ctl" class:off={!bursts}>
                <div class="row">
                  <span class="label" id="cull-gap">{t('cull.gap')}</span>
                  <output>{t('cull.seconds', { value: gap })}</output>
                </div>
                <input type="range" min="1" max="30" step="1" bind:value={gap} disabled={!bursts}
                       aria-labelledby="cull-gap" />
                <small>{t('cull.gapHint')}</small>
              </div>
            </div>
          {/if}
        </section>

        <p class="guide">
          {t('cull.toggleHint')}
          {#if notRaw > 0}<span class="warn">{t('cull.notRaw', { count: notRaw })}</span>{/if}
        </p>

        <ul class="gallery">
          {#each ordered as group (group[0].uid)}
            {@const unfolded = open.has(group[0].uid)}
            {#each unfolded ? group : group.slice(0, 1) as it, nth (it.uid)}
              <li class:picked={isPicked(it)} class:follower={nth > 0}>
                <button class="photo" onclick={() => toggle(it)} title={it.name}>
                  <img src={it.thumb} alt={it.name} loading="lazy" />
                  <span class="score" title={why(it)}>{mark(ranking.marks.get(it)?.score ?? blend(it, weight))}</span>
                  {#if isPicked(it)}<span class="tick">{t('cull.picked')}</span>{/if}
                </button>
                <div class="caption">
                  <span class="name">{it.name}</span>
                  <span class="marks" title={t('cull.marksTitle',
                                               { a: mark(it.aesthetic), t: mark(it.technical) })}>
                    {t('cull.marks', { a: mark(it.aesthetic), t: mark(it.technical) })}
                  </span>
                </div>
                <div class="measures" title={t('cull.measuresTitle',
                                               { brightness: share(it.brightness) })}>
                  <span class:warn={it.blown >= CLIPPED}>
                    {t('cull.blown', { share: share(it.blown) })}
                  </span>
                  <span class:warn={it.crushed >= CLIPPED}>
                    {t('cull.black', { share: share(it.crushed) })}
                  </span>
                  <span>{t('cull.sharp', { value: n(sharp.get(it) ?? 0, 0) })}</span>
                  {#if it.faces?.length}
                    {@const shutCount = blinking(it)}
                    <span class:warn={shutCount > 0}>
                      {shutCount > 0
                        ? t('cull.eyesShut', { count: shutCount })
                        : t('cull.faces', { count: it.faces.length })}
                    </span>
                  {/if}
                </div>
                {#if nth === 0 && group.length > 1}
                  <button class="ghost small more" onclick={() => unfold(it)}>
                    {unfolded ? t('cull.fewer') : t('cull.similar', { count: group.length - 1 })}
                  </button>
                {/if}
              </li>
            {/each}
          {/each}
        </ul>
      {/if}
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../../styles/breakpoints' as *;
  @use '../../../styles/surfaces' as *;
  @use '../../../styles/type' as *;

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

  /* The words keep a reading width while the gallery under them is wide. */
  .intro { max-width: 880px; }
  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  .how {
    font-size: 13.5px; color: var(--color-muted);
    summary { cursor: pointer; color: var(--color-accent); }
    p { margin: 8px 0 0; line-height: 1.6; }
  }

  /* The rest of `.drop` is in app.scss. */
  .drop {
    min-height: 200px;
    &.compact { min-height: 100px; }
    &.disabled { opacity: .5; pointer-events: none; }
    strong { font-size: 18px; }
    span { text-align: center; padding: 0 12px; }
  }

  .error {
    margin: 0; padding: 10px 12px; border-radius: 8px;
    border: 1px solid var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
    color: var(--color-ink); font-size: 13.5px;
  }
  .note { margin: 0; font-size: 13px; color: var(--color-muted); overflow-wrap: anywhere; }

  .panel {
    @include surface;
    padding: 16px 18px;
  }

  .status {
    display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 8px 16px;
    p { margin: 0; font-size: 14px; font-variant-numeric: tabular-nums; }
    .meter { grid-row: 2; }
    button { grid-row: 1 / span 2; grid-column: 2; }
  }
  .meter {
    height: 4px; border-radius: 2px; background: var(--color-line); overflow: hidden;
    div { height: 100%; background: var(--color-accent); transition: width .3s ease-out; }
  }

  /* -- the toolbar ------------------------------------------------------ */

  .toolbar {
    @include surface;
    position: sticky; top: -22px; z-index: 4;
    padding: 12px 16px 14px;
    display: grid; gap: 12px;
    box-shadow: 0 10px 24px -14px rgb(0 0 0 / 70%);
    @include phone { top: -12px; padding: 10px 12px; gap: 10px; }
  }

  .head {
    display: flex; align-items: center; gap: 12px;
    @include phone { flex-wrap: wrap; gap: 8px; }
  }
  .tally {
    margin: 0; display: flex; align-items: baseline; gap: 6px; min-width: 0;
    font-size: 13.5px; color: var(--color-muted); font-variant-numeric: tabular-nums;
    strong { font-size: 22px; font-weight: 650; color: var(--color-accent); line-height: 1; }
  }
  .actions {
    margin-left: auto; display: flex; gap: 8px;
    button { padding: 7px 14px; }
    @include phone {
      order: 3; width: 100%; margin-left: 0;
      button { padding: 8px 12px; min-height: 40px; }
      .develop { flex: 1; }
    }
  }
  /* Phones only: the controls fold behind it. */
  .adjust {
    display: none; margin-left: auto; padding: 6px 12px; min-height: 36px; font-size: 13px;
    &[aria-expanded='true'] { color: var(--color-ink); border-color: var(--color-accent); }
    @include phone { display: inline-block; }
  }

  .tune {
    display: grid; grid-template-columns: minmax(0, 1.3fr) auto minmax(0, 1.1fr);
    gap: 0; padding-top: 12px; border-top: 1px solid var(--color-line);
    @include narrow {
      grid-template-columns: 1fr 1fr;
      .ctl:first-child { grid-column: 1 / -1; border-bottom: 1px solid var(--color-line); }
    }
    @include phone {
      display: none; grid-template-columns: 1fr;
      .tuning & { display: grid; }
      .ctl, .ctl:first-child { grid-column: auto; border-bottom: 1px solid var(--color-line); }
      .ctl:last-child { border-bottom: 0; }
    }
  }
  .ctl {
    display: grid; gap: 6px; align-content: start; padding: 2px 18px 4px;
    & + & { border-left: 1px solid var(--color-line); }
    &:first-child { padding-left: 0; }
    &:last-child { padding-right: 0; }
    @include narrow {
      padding: 8px 14px;
      &:first-child { padding: 0 0 10px; border-left: 0; }
      &:nth-child(2) { padding-left: 0; border-left: 0; }
    }
    @include phone {
      padding: 10px 0 !important; border-left: 0 !important;
    }
  }
  .label { @include section-label; }

  /* The levers past the first three, behind a quiet link. */
  .more {
    display: flex; justify-content: space-between; gap: 12px; margin-top: -4px;
    @include phone {
      display: none;
      .tuning & { display: flex; }
    }
  }
  .linkish {
    background: none; border: 0; padding: 2px 0; min-height: 0; font-weight: 500;
    font-size: 12.5px; color: var(--color-accent);
    @include phone { padding: 8px 0; font-size: 13.5px; }
    &:hover { filter: none; text-decoration: underline; }
    .caret { display: inline-block; margin-left: 4px; transition: transform .15s; }
    &[aria-expanded='true'] .caret { transform: rotate(180deg); }
  }
  .refine {
    grid-template-columns: repeat(5, minmax(0, 1fr));
    .ctl { padding-bottom: 0; }
    .row { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
    output { font-size: 12px; color: var(--color-ink); font-variant-numeric: tabular-nums; }
    input[type='range'] { margin: 2px 0; }
    small { font-size: 11.5px; line-height: 1.4; color: var(--color-muted); }
    .off { opacity: .4; }
    @include narrow {
      grid-template-columns: 1fr 1fr;
      .ctl:first-child { grid-column: auto; border-bottom: 0; padding: 8px 14px 8px 0; }
      .ctl:nth-child(even) { padding-left: 14px; border-left: 1px solid var(--color-line); }
      .ctl:nth-child(odd) { padding-left: 0; border-left: 0; }
    }
    @include phone {
      grid-template-columns: 1fr;
      .ctl, .ctl:first-child, .ctl:nth-child(even), .ctl:nth-child(odd) {
        padding: 10px 0 !important; border-left: 0 !important;
        border-bottom: 1px solid var(--color-line);
      }
      .ctl:last-child { border-bottom: 0; }
    }
  }

  .slide {
    display: flex; align-items: center; gap: 10px;
    &.off { opacity: .4; }
    input[type='range'] { flex: 1; min-width: 60px; margin: 0; }
    .end { font-size: 12px; color: var(--color-muted); white-space: nowrap; }
  }

  .keep { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .stepper, .seg {
    display: inline-flex; border: 1px solid var(--color-line); border-radius: 8px;
    overflow: hidden; background: var(--color-panel-2);
    button {
      padding: 0 11px; min-height: 32px; border-radius: 0; font-weight: 500;
      background: transparent; color: var(--color-muted);
      @include phone { min-height: 40px; padding: 0 14px; }
      &:hover { color: var(--color-ink); filter: none; }
    }
  }
  .stepper {
    button { font-size: 16px; line-height: 1; }
    input {
      width: 52px; border: 0; border-inline: 1px solid var(--color-line); border-radius: 0;
      background: transparent; color: var(--color-ink); text-align: center;
      font: inherit; font-size: 14px; font-variant-numeric: tabular-nums;
      appearance: textfield; -moz-appearance: textfield;
      &::-webkit-inner-spin-button, &::-webkit-outer-spin-button { appearance: none; margin: 0; }
    }
  }
  .seg button {
    font-size: 12.5px;
    & + button { border-left: 1px solid var(--color-line); }
    &[aria-checked='true'] {
      color: var(--color-accent-ink); background: var(--color-accent);
    }
  }

  /* A switch rather than a checkbox: it turns a behaviour on. */
  .switch {
    display: flex; align-items: center; gap: 8px; cursor: pointer; width: fit-content;
    input {
      appearance: none; margin: 0; flex: 0 0 auto; cursor: pointer;
      width: 28px; height: 16px; border-radius: 999px; position: relative;
      background: var(--color-line); transition: background .15s;
      &::before {
        content: ''; position: absolute; top: 2px; left: 2px;
        width: 12px; height: 12px; border-radius: 50%;
        background: var(--color-ink); transition: transform .15s;
      }
      &:checked { background: var(--color-accent); }
      &:checked::before { transform: translateX(12px); background: var(--color-accent-ink); }
      &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
      /* Big enough for a thumb. */
      @include phone {
        width: 40px; height: 24px;
        &::before { top: 3px; left: 3px; width: 18px; height: 18px; }
        &:checked::before { transform: translateX(16px); }
      }
    }
    @include phone { min-height: 40px; }
  }

  .guide {
    margin: -6px 2px 0; font-size: 12.5px; color: var(--color-muted);
    .warn { margin-left: 8px; color: var(--color-accent); }
  }

  button.small { padding: 4px 10px; min-height: 0; font-size: 12px; }

  .gallery {
    list-style: none; margin: 0; padding: 0;
    display: grid; gap: 14px;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    @include phone { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }

    li { display: grid; gap: 6px; align-content: start; min-width: 0; }
    /* The other frames of a moment, once unfolded: the same card, quieter. */
    li.follower .photo { opacity: .8; }
  }

  .photo {
    position: relative; display: block; width: 100%; padding: 0; min-height: 0; font-weight: 400;
    aspect-ratio: 3 / 2; border-radius: 10px; overflow: hidden; cursor: pointer;
    background: var(--color-panel-2); border: 2px solid transparent;
    img { width: 100%; height: 100%; object-fit: contain; display: block; }
    .picked & { border-color: var(--color-accent); }
  }
  .score, .tick {
    position: absolute; top: 6px; padding: 2px 7px; border-radius: 999px;
    font-size: 12px; font-weight: 600; font-variant-numeric: tabular-nums;
    background: rgb(13 14 16 / 80%); color: #fff;
  }
  .score { left: 6px; }
  .tick { right: 6px; background: var(--color-accent); color: var(--color-accent-ink); }

  .caption {
    display: flex; justify-content: space-between; gap: 8px; font-size: 12px;
    .name {
      min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
      color: var(--color-ink);
    }
    .marks { color: var(--color-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  }
  /* How it is exposed and how sharp it is, under the marks; a share worth
     a second look in the accent. */
  .measures {
    display: flex; flex-wrap: wrap; gap: 2px 10px; margin-top: -3px;
    font-size: 11.5px; color: var(--color-muted); font-variant-numeric: tabular-nums;
    cursor: help;
    .warn { color: var(--color-accent); }
  }
  .more { justify-self: start; }
</style>
