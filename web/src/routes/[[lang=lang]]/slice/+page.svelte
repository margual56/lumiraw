<script>
  /** One photograph in, strips of it out: columns side by side or rows one
   *  above the other, a number of equal ones or a set size each, for a
   *  carousel or a print in panels. The cutting is done here, by the browser;
   *  a raw is developed by the module first. */
  import Progress from '$lib/components/Progress.svelte';
  import SiteHeader from '$lib/components/SiteHeader.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import * as api from '$lib/api.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { errorText, isRaw, RAW_ACCEPT, saveFile } from '$lib/format.js';
  import { recall, fileKey } from '$lib/memory.js';
  import { app, defaultSettings, resetForNewPhoto, usable, UPLOAD } from '$lib/state.svelte.js';
  import { MAX_SLICES, cuts, outputFor, strips, stripName, takeHandOff } from '$lib/slice.js';
  import { stamp } from '$lib/wasm/stamp.js';
  import { zip } from '$lib/zip.js';

  const IMAGE_ACCEPT = 'image/jpeg,image/png,image/webp,image/avif,.jpg,.jpeg,.png,.webp,.avif';

  /** The photograph being sliced: `{ name, type, bitmap, url, width, height,
   *  captured }`, the bitmap already the right way up. */
  let picture = $state(null);
  let across = $state('columns');
  let by = $state('count');
  let count = $state(3);
  let size = $state(1080);
  let quality = $state(92);
  /** What the strips are written as: the picture's own format, or JPEG for
   *  a raw, until another is chosen. */
  let format = $state('image/jpeg');
  let error = $state('');
  let saved = $state('');
  let over = $state(false);
  let cutting = $state(null);   // { n, total } while the strips are written
  let input;

  // WebP is written by the browser, and not every engine can.
  const webp = typeof document !== 'undefined'
    && document.createElement('canvas').toDataURL('image/webp').startsWith('data:image/webp');
  const FORMATS = $derived([['image/jpeg', 'JPEG'], ['image/png', 'PNG'],
                            ...(webp ? [['image/webp', 'WebP']] : [])]);
  const output = $derived(outputFor(format, { webp }));
  const lossy = $derived(output && output.type !== 'image/png');

  const how = $derived({ across, by, count, size });
  const pieces = $derived(picture ? strips(picture.width, picture.height, how) : []);
  const side = $derived(picture ? (across === 'rows' ? picture.height : picture.width) : 0);

  /** The cuts as fractions of the picture's side, for the lines over it. */
  const lines = $derived(picture
    ? cuts(side, how).slice(1).map(([start]) => start / side) : []);

  /** What the strips will be, in a sentence. */
  const summary = $derived.by(() => {
    if (!pieces.length) return '';
    if (pieces.length === 1) return t('slice.summary.one');
    const lengths = pieces.map((p) => (across === 'rows' ? p.height : p.width));
    const first = lengths[0];
    const last = lengths[lengths.length - 1];
    if (by === 'size') {
      if (first > Math.max(1, Math.floor(size))) {
        return t('slice.summary.capped', { count: MAX_SLICES, size: n(first, 0) });
      }
      return last === first
        ? t('slice.summary.even', { count: pieces.length, size: n(first, 0) })
        : t('slice.summary.rest', { count: pieces.length, size: n(first, 0), rest: n(last, 0) });
    }
    const [small, big] = [Math.min(...lengths), Math.max(...lengths)];
    return small === big
      ? t('slice.summary.even', { count: pieces.length, size: n(small, 0) })
      : t('slice.summary.odd', { count: pieces.length, size: n(small, 0), other: n(big, 0) });
  });

  // Only this page's own work moves the bar: a preview the developer left
  // running reports to whichever page installed the listener, and here it
  // would read as something this page is doing.
  api.setProgressListener(() => {});
  app.progress = null;

  $effect(() => {
    // A photograph sent over from the developer's download step.
    const handed = takeHandOff();
    if (handed) {
      void show(handed.blob, handed.name, handed.captured);
    }
    return () => forget();
  });

  function forget() {
    if (!picture) return;
    picture.bitmap.close?.();
    URL.revokeObjectURL(picture.url);
    picture = null;
  }

  /** Put a picture on the page, from a file or a blob, the right way up. */
  async function show(blob, name, captured = null, type = blob.type) {
    const bitmap = await createImageBitmap(blob, { imageOrientation: 'from-image' });
    forget();
    picture = { name, bitmap, url: URL.createObjectURL(blob),
                width: bitmap.width, height: bitmap.height, captured };
    format = outputFor(type, { webp }).type;
    // A set size bigger than the side would make one strip; start at a half.
    if (size >= side) size = Math.max(1, Math.round(side / 2));
  }

  async function take(files) {
    over = false;
    error = '';
    saved = '';
    const file = Array.from(files ?? [])[0];
    if (input) input.value = '';
    if (!file || app.busy) return;
    try {
      if (isRaw(file.name)) {
        await developRaw(file);
      } else {
        await show(file, file.name);
      }
    } catch (err) {
      error = err?.name === 'InvalidStateError' || err?.name === 'EncodingError'
        ? t('slice.unreadable') : errorText(err);
    }
  }

  /** Develop a raw with the settings it was last given here, or the automatic
   *  ones, at full size and losslessly, and show that. The module holds one
   *  photograph at a time, so the developer's is put back afterwards. */
  async function developRaw(file) {
    const settings = usable(recall(fileKey(file))) ?? defaultSettings();
    const back = app.info ? app.roll[app.rollAt]?.file : null;
    app.busy = true;
    app.busyKey = 'slice.developing';
    app.busyParams = { file: file.name };
    try {
      await api.upload(file, { deep: settings.deep_denoise });
      const out = await api.exportOnce({
        settings, format: 'png8', quality: 100, max_size: null, original_name: file.name,
      }, (fraction, stage) => (app.progress = { fraction, code: stage }));
      // Developed losslessly, so the strips are encoded once; written as
      // JPEG unless another format is chosen.
      await show(out.blob, file.name, out.captured, 'image/jpeg');
    } finally {
      if (back) {
        await api.upload(back, { deep: app.settings.deep_denoise }).catch(() => {});
      } else if (app.info) {
        // A merged picture has no file to open again; the developer starts over
        // rather than show one photograph and develop another.
        resetForNewPhoto();
        app.info = null;
        app.step = UPLOAD;
      }
      app.busy = false;
      app.busyKey = '';
      app.progress = null;
    }
  }

  /** Cut the strips, one canvas each, and save them in one zip. */
  async function run() {
    if (!picture || !pieces.length) return;
    error = '';
    saved = '';
    const files = [];
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    try {
      for (const [i, p] of pieces.entries()) {
        cutting = { n: i + 1, total: pieces.length };
        canvas.width = p.width;
        canvas.height = p.height;
        if (output.type === 'image/jpeg') {
          // JPEG has no transparency; a transparent PNG would turn black.
          ctx.fillStyle = '#fff';
          ctx.fillRect(0, 0, p.width, p.height);
        } else {
          ctx.clearRect(0, 0, p.width, p.height);
        }
        ctx.drawImage(picture.bitmap, p.x, p.y, p.width, p.height, 0, 0, p.width, p.height);
        const blob = await new Promise((resolve, reject) => canvas.toBlob(
          (b) => (b ? resolve(b) : reject(new Error(t('slice.unreadable')))),
          output.type, lossy ? quality / 100 : undefined));
        files.push({ name: stripName(picture.name, i, pieces.length, output.ext), blob,
                     captured: picture.captured });
      }
      const name = `${picture.name.replace(/\.[^.]+$/, '') || 'photo'}_strips.zip`;
      const blob = await zip(files, stamp());
      saveFile(blob, name);
      saved = t('slice.saved', { name, count: files.length, size: n(blob.size / 1e6, 1) });
    } catch (err) {
      error = errorText(err);
    } finally {
      cutting = null;
      canvas.width = canvas.height = 0;
    }
  }

  function clear() {
    forget();
    error = '';
    saved = '';
  }
</script>

<Meta title={t('seo.slice.title')} description={t('seo.slice.description')} path="/slice" />

<div class="shell">
  <SiteHeader tagline={t('slice.title')} />
  <Progress />

  <main>
    <div class="mx-auto grid max-w-[880px] gap-4">
      <h1 class="pagetitle">{t('slice.heading')}</h1>
      <p class="lead">{t('slice.lead')}</p>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <label
        class="drop"
        class:compact={!!picture}
        class:over
        ondragenter={(e) => { e.preventDefault(); over = true; }}
        ondragover={(e) => { e.preventDefault(); over = true; }}
        ondragleave={() => (over = false)}
        ondrop={(e) => { e.preventDefault(); take(e.dataTransfer.files); }}
      >
        <input
          bind:this={input}
          type="file"
          accept={`${IMAGE_ACCEPT},${RAW_ACCEPT}`}
          onchange={(e) => take(e.currentTarget.files)}
        />
        <strong>
          <span class="on-hover">{t(picture ? 'slice.dropAnother' : 'slice.drop')}</span>
          <span class="on-touch">{t(picture ? 'slice.dropAnother.touch' : 'slice.drop.touch')}</span>
        </strong>
        <span>{t('slice.formats')}</span>
        {#if app.busy && app.busyKey === 'slice.developing'}
          <em>{t('slice.developing', app.busyParams)}</em>
        {/if}
      </label>

      {#if error}<p class="error">{error}</p>{/if}

      {#if picture}
        <section class="panel">
          <div class="preview">
            <div class="frame">
              <img src={picture.url} alt={picture.name} />
              {#each lines as at (at)}
                <span class="cut" class:rows={across === 'rows'}
                      style={across === 'rows' ? `top: ${at * 100}%` : `left: ${at * 100}%`}></span>
              {/each}
            </div>
          </div>
          <p class="about">{t('slice.picture', { width: n(picture.width, 0),
                                                height: n(picture.height, 0), name: picture.name })}</p>
        </section>

        <section class="panel">
          <h2>{t('slice.across')}</h2>
          <div class="choices" role="radiogroup" aria-label={t('slice.across')}>
            {#each ['columns', 'rows'] as way (way)}
              <button type="button" role="radio" aria-checked={across === way}
                      class="choice" class:on={across === way} onclick={() => (across = way)}>
                <strong>{t(`slice.across.${way}`)}</strong>
                <span>{t(`slice.across.${way}.hint`)}</span>
              </button>
            {/each}
          </div>

          <div class="field">
            <label for="slice-by">{t('slice.by')}</label>
            <select id="slice-by" bind:value={by}>
              <option value="count">{t('slice.by.count')}</option>
              <option value="size">{t(across === 'rows' ? 'slice.by.size.rows' : 'slice.by.size')}</option>
            </select>
          </div>

          {#if by === 'count'}
            <div class="field">
              <label for="slice-count">{t('slice.count')}</label>
              <input id="slice-count" type="number" min="1" max={MAX_SLICES} step="1"
                     bind:value={count} />
            </div>
          {:else}
            <div class="field">
              <label for="slice-size">{t('slice.size')}</label>
              <input id="slice-size" type="number" min="1" max={side} step="1" bind:value={size} />
            </div>
          {/if}

          <div class="field">
            <label for="slice-format">{t('slice.format')}</label>
            <select id="slice-format" bind:value={format}>
              {#each FORMATS as [type, label] (type)}
                <option value={type}>{label}</option>
              {/each}
            </select>
          </div>

          {#if lossy}
            <div class="field">
              <div class="row">
                <label for="slice-quality">{t('slice.quality')}</label>
                <output for="slice-quality">{quality}</output>
              </div>
              <input id="slice-quality" type="range" min="60" max="100" step="1"
                     bind:value={quality} />
            </div>
          {/if}

          <p class="summary">{summary}</p>

          <div class="flex flex-wrap gap-2.5">
            <button onclick={run} disabled={!!cutting || app.busy || !pieces.length}>
              {cutting ? t('slice.running', cutting) : t('slice.run')}
            </button>
            <button class="ghost" onclick={clear} disabled={!!cutting}>{t('slice.clear')}</button>
          </div>
          {#if saved}<p class="saved">{saved}</p>{/if}
        </section>
      {/if}
    </div>
  </main>
</div>

<style lang="scss">
  @use '../../../styles/breakpoints' as *;
  @use '../../../styles/surfaces' as *;

  .lead { margin: 0; font-size: 15px; line-height: 1.6; color: var(--color-muted); }

  /* The rest of `.drop` is in app.scss. */
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
  }

  /* The picture as large as fits, with the cuts drawn over it where they
     will fall. */
  .preview { display: flex; justify-content: center; }
  .frame { position: relative; line-height: 0; }
  .frame img {
    display: block; max-width: 100%; max-height: 62vh; width: auto; height: auto;
    border-radius: 4px;
  }
  .cut {
    position: absolute; top: 0; bottom: 0; width: 0;
    border-left: 2px dashed var(--color-accent);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 35%);
    transform: translateX(-1px);
    pointer-events: none;
    &.rows {
      top: auto; bottom: auto; left: 0; right: 0; width: auto; height: 0;
      border-left: 0; border-top: 2px dashed var(--color-accent);
      transform: translateY(-1px);
    }
  }
  .about {
    margin: 10px 0 0; text-align: center; font-size: 12.5px; color: var(--color-muted);
    overflow-wrap: anywhere;
  }

  .choices {
    display: grid; gap: 8px; grid-template-columns: 1fr 1fr;
  }
  .choice {
    @include surface;
    display: grid; gap: 3px; align-content: start; text-align: left;
    padding: 10px 12px; min-height: 0;
    background: var(--color-panel); color: var(--color-ink);
    box-shadow: inset 0 0 0 1px var(--color-line);
    strong { font-size: 13.5px; font-weight: 650; }
    span { font-size: 12px; line-height: 1.4; color: var(--color-muted); font-weight: 400; }
    &:hover { box-shadow: inset 0 0 0 1px var(--color-accent); }
    &.on {
      box-shadow: inset 0 0 0 2px var(--color-accent);
      strong { color: var(--color-accent); }
    }
  }

  .field {
    select, input[type='number'] { display: block; width: 100%; max-width: 280px; margin-top: 6px; }
    input[type='range'] { width: 100%; max-width: 280px; accent-color: var(--color-accent); }
    .row { max-width: 280px; }
  }

  .summary {
    margin: 4px 0 14px; font-size: 13.5px; color: var(--color-ink);
    font-variant-numeric: tabular-nums;
  }
  .saved { margin: 10px 0 0; font-size: 13px; color: var(--color-ok); }
</style>
