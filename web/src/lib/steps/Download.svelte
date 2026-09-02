<script>
  import Viewer from '$lib/components/Viewer.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { NAME } from '$lib/brand.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { lookLabel, formatLabel, stageText, errorText } from '$lib/format.js';
  import { startExport, jobStatus, jobFile } from '$lib/api.js';
  import { exportRoll } from '$lib/roll.svelte.js';

  let format = $state('png8');
  let quality = $state(92);
  let maxSize = $state('');
  let working = $state(false);

  const lossy = $derived(['jpeg', 'webp'].includes(format));
  // One photograph, one file.
  const summary = $derived(t('download.one', { name: lookLabel(app.settings.curves.look) }));

  const SIZES = ['', '4000', '2560', '1600'];

  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  function save(blob, name) {
    const url = URL.createObjectURL(blob);
    const link = Object.assign(document.createElement('a'), { href: url, download: name });
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
  }

  const kept = $derived(app.roll.filter((item) => item.keep).length);
  let batch = $state(null);      // { text, progress } while the roll exports

  /** Every photograph marked in the filmstrip, developed with its own
   *  settings (or the current grade, if never opened), in one zip. */
  async function downloadRoll() {
    working = true;
    app.busy = true;
    app.exportState = '';
    const started = performance.now();
    try {
      const { blob, count, failed } = await exportRoll({
        format, quality, max_size: maxSize || null,
        onstatus: (text, progress) => (batch = { text, progress }),
      });
      const first = (app.roll.find((item) => item.keep)?.name ?? NAME).replace(/\.[^.]+$/, '');
      save(blob, `${first}_and_${count - 1}_more.zip`);
      app.exportState = t('roll.saved', {
        count, size: n(blob.size / 1e6, 1), seconds: n((performance.now() - started) / 1000, 0),
      }) + (failed.length ? `\n${t('roll.failed')}\n${failed.join('\n')}` : '');
    } catch (err) {
      app.exportState = errorText(err);
    } finally {
      working = false;
      app.busy = false;
      batch = null;
    }
  }

  async function download() {
    working = true;
    app.exportState = '';
    app.job = { progress: 0, stage: 'starting', stage_params: {}, elapsed: 0 };
    try {
      const { job } = await startExport({
        id: app.id,
        settings: app.settings,
        format,
        quality,
        max_size: maxSize || null,
        original_name: app.info.original_name,
      });

      // The server reports the pipeline's real stages; poll until it stops.
      let status;
      do {
        await wait(400);
        status = await jobStatus(job);
        app.job = {
          progress: status.progress,
          stage: status.stage,
          stage_params: status.stage_params ?? {},
          elapsed: status.elapsed,
        };
      } while (status.state === 'running');
      if (status.state === 'error') throw new Error(status.error);

      const res = await jobFile(job);
      const blob = await res.blob();
      const match = (res.headers.get('Content-Disposition') || '').match(/filename="(.+?)"/);
      const name = match ? match[1] : `${NAME.toLowerCase()}.png`;
      save(blob, name);
      app.exportState = t('download.saved', {
        name, size: n(blob.size / 1e6, 1), seconds: n(status.elapsed, 0),
      });
    } catch (err) {
      app.exportState = errorText(err);
    } finally {
      working = false;
      app.job = null;
    }
  }
</script>

<div class="stage">
  <Viewer src={app.output.image} busy={app.busy} />
  <aside>
    <h2>{t('download.title')}</h2>

    <div class="field">
      <label for="format">{t('download.format')}</label>
      <select id="format" bind:value={format}>
        {#each app.info?.formats ?? [] as option}
          <option value={option.id}>{formatLabel(option.id)}</option>
        {/each}
      </select>
    </div>

    {#if lossy}
      <Slider label={t('download.quality')} bind:value={quality}
              min={60} max={100} step={1} format={(v) => String(v)} />
    {/if}

    <div class="field">
      <label for="maxsize">{t('download.longEdge')}</label>
      <select id="maxsize" bind:value={maxSize}>
        {#each SIZES as size}
          <option value={size}>
            {size ? t('download.px', { size: n(Number(size), 0) }) : t('download.full')}
          </option>
        {/each}
      </select>
    </div>

    <p class="lede">{summary}</p>
    <button class="go" onclick={download} disabled={working}>
      {working ? t('download.working') : t('download.button')}
    </button>

    {#if app.roll.length > 1}
      <button class="ghost roll" onclick={downloadRoll} disabled={working || !kept}>
        {t('roll.button', { count: kept })}
      </button>
      <p class="hint small">{t('roll.explain')}</p>
    {/if}

    {#if batch}
      <div class="job">
        <div class="track"><div class="fill" style:width={`${batch.progress * 100}%`}></div></div>
        <div class="row"><span>{batch.text}</span><span>{n(batch.progress * 100, 0)} %</span></div>
      </div>
    {/if}

    {#if app.job}
      <div class="job">
        <div class="track"><div class="fill" style:width={`${app.job.progress * 100}%`}></div></div>
        <div class="row">
          <span>{stageText(app.job.stage, app.job.stage_params)}</span>
          <span>{n(app.job.progress * 100, 0)} % · {n(app.job.elapsed, 0)} s</span>
        </div>
      </div>
    {/if}

    <em class="hint result">{app.exportState}</em>
  </aside>
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .job { margin-top: 14px; }
  /* The one thing this step is for, as wide as the thumb that presses it. */
  .go { @include phone { width: 100%; min-height: 48px; } }
  .roll { margin-top: 8px; width: 100%; }
  .small { display: block; margin-top: 4px; @include small(11.5px); }
  .hint { white-space: pre-line; }
  .track {
    height: 5px; border-radius: 3px; background: var(--color-panel-2); overflow: hidden;
    .fill { height: 100%; background: var(--color-accent); transition: width .3s ease-out; }
  }
  .row {
    display: flex; justify-content: space-between; gap: 10px; margin-top: 6px;
    @include small(11.5px); color: var(--color-muted); font-variant-numeric: tabular-nums;
    /* The stage name can be long in either language; the elapsed time beside
       it must not be pushed off the end. */
    span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  }
</style>
