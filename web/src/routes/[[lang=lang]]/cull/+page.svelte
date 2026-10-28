<script>
  /** Culling: a whole shoot in, every photo scored by NIMA, the best of each
   *  moment picked and handed to the developer as a roll. */
  import { goto } from '$app/navigation';
  import Progress from '$lib/components/Progress.svelte';
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import * as api from '$lib/api.js';
  import * as critic from '$lib/critic.js';
  import { blend, centred, groupBursts, pick, relativeSharpness, toCsv, CLIPPED } from '$lib/cull.js';
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

  let weight = $state(0.6);
  let keepMode = $state('percent');
  let keepValue = $state(10);
  let bursts = $state(true);
  let similarity = $state(0.8);
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

  const units = $derived(centred(shown));
  const groups = $derived(bursts
    ? groupBursts(shown, { similarity, units })
    : shown.map((it) => [it]));
  const ranking = $derived(pick(groups, weight, { mode: keepMode, value: keepValue }, shown.length));
  const sharp = $derived(relativeSharpness(shown));
  const isPicked = (it) => (overrides.has(it.uid) ? overrides.get(it.uid) : ranking.picked.has(it));
  /** Every group, its best frame first, the best groups first. */
  const ordered = $derived([...ranking.groups]
    .sort((a, b) => blend(b[0], weight) - blend(a[0], weight)));
  const pickedCount = $derived(shown.filter(isPicked).length);
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
    const blob = new Blob([toCsv(ranking.groups, picked, weight, sharp)], { type: 'text/csv' });
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
        <section class="panel controls">
          <h2>{t('cull.controls')}</h2>
          <div class="choices">
            <div class="option">
              <div class="row">
                <strong>{t('cull.weight')}</strong>
                <span class="value">{n(weight * 100, 0)} / {n((1 - weight) * 100, 0)}</span>
              </div>
              <input type="range" min="0" max="1" step="0.05" bind:value={weight} />
              <div class="ends">
                <span>{t('cull.weight.technical')}</span>
                <span>{t('cull.weight.aesthetic')}</span>
              </div>
            </div>

            <div class="option">
              <div class="row"><strong>{t('cull.keep')}</strong></div>
              <div class="keep">
                <input type="number" min="1" max={keepMode === 'percent' ? 100 : shown.length}
                       bind:value={keepValue} />
                <select bind:value={keepMode}>
                  <option value="count">{t('cull.keep.count')}</option>
                  <option value="percent">{t('cull.keep.percent')}</option>
                </select>
              </div>
            </div>

            <!-- The labelled checkbox from app.scss. -->
            <label class="check">
              <input type="checkbox" bind:checked={bursts} />
              <span>{t('cull.bursts')}<em>{t('cull.burstsHint')}</em></span>
            </label>

            <div class="option" class:off={!bursts}>
              <div class="row"><strong>{t('cull.similarity')}</strong></div>
              <input type="range" min="0.6" max="0.95" step="0.01" disabled={!bursts}
                     bind:value={similarity} />
              <div class="ends">
                <span>{t('cull.similarity.loose')}</span>
                <span>{t('cull.similarity.strict')}</span>
              </div>
            </div>
          </div>

          <p class="summary">
            {t('cull.summary', { picked: pickedCount, total: shown.length, groups: groups.length })}
            <span>{t('cull.toggleHint')}</span>
          </p>
          <div class="flex flex-wrap gap-2.5">
            <button onclick={develop} disabled={running || app.busy}>{t('cull.develop')}</button>
            <button class="ghost" onclick={csv}>{t('cull.csv')}</button>
            {#if !running}<button class="ghost" onclick={clear}>{t('cull.again')}</button>{/if}
          </div>
          <small class="hint">{t('cull.developHint')}</small>
        </section>

        <ul class="gallery">
          {#each ordered as group (group[0].uid)}
            {@const unfolded = open.has(group[0].uid)}
            {#each unfolded ? group : group.slice(0, 1) as it, nth (it.uid)}
              <li class:picked={isPicked(it)} class:follower={nth > 0}>
                <button class="photo" onclick={() => toggle(it)} title={it.name}>
                  <img src={it.thumb} alt={it.name} loading="lazy" />
                  <span class="score">{mark(blend(it, weight))}</span>
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
    h2 { margin: 0 0 12px; font-size: 15px; font-weight: 650; }
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

  .choices {
    display: grid; grid-template-columns: 1fr 1fr; gap: 18px 36px; align-items: start;
    @include phone { grid-template-columns: 1fr; }
    /* The shared checkbox carries margins meant for a side panel. */
    .check { margin: 0; }
  }

  .option {
    display: grid; gap: 4px; align-content: start;
    strong { font-size: 14px; font-weight: 600; }
    &.off { opacity: .5; }
    .row { display: flex; justify-content: space-between; align-items: baseline; }
    .value { color: var(--color-muted); font-size: 12.5px; font-variant-numeric: tabular-nums; }
    input[type='range'] { width: 100%; accent-color: var(--color-accent); }
    .ends {
      display: flex; justify-content: space-between;
      color: var(--color-muted); font-size: 12px;
    }
  }
  .keep {
    display: flex; gap: 8px;
    input, select {
      min-height: 34px; padding: 4px 8px; border-radius: 8px; font: inherit; font-size: 14px;
      background: var(--color-panel-2); color: var(--color-ink);
      border: 1px solid var(--color-line);
    }
    input { width: 90px; font-variant-numeric: tabular-nums; }
    select { width: auto; margin-top: 0; }
  }

  .summary {
    margin: 16px 0 12px; font-size: 14px;
    span { margin-left: 6px; color: var(--color-muted); font-size: 13px; }
  }
  .hint { display: block; margin-top: 10px; color: var(--color-muted); font-size: 12.5px; }

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
