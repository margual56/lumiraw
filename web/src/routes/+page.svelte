<script>
  import Stepper from '$lib/components/Stepper.svelte';
  import Progress from '$lib/components/Progress.svelte';
  import Dropzone from '$lib/components/Dropzone.svelte';
  import Privacy from '$lib/components/Privacy.svelte';
  import Framing from '$lib/steps/Framing.svelte';
  import Brightness from '$lib/steps/Brightness.svelte';
  import WhiteBalance from '$lib/steps/WhiteBalance.svelte';
  import Vibrance from '$lib/steps/Vibrance.svelte';
  import BeforeAfter from '$lib/steps/BeforeAfter.svelte';
  import Looks from '$lib/steps/Looks.svelte';
  import Download from '$lib/steps/Download.svelte';

  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import * as api from '$lib/api.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import {
    app, resetForNewPhoto, settingsKey,
    STEPS, UPLOAD, FRAME, EXPOSURE, WB, VIBRANCE, COMPARE, LOOKS, DOWNLOAD,
  } from '$lib/state.svelte.js';

  const fileLabel = $derived(app.info
    ? [app.info.original_name,
       `${app.info.width}×${app.info.height}`,
       app.info.profile.camera,
       app.info.profile.lens || t('profile.unknownLens'),
       `ISO ${app.info.profile.iso}`,
       `f/${app.info.profile.aperture}`].join(' · ')
    : '');

  // The pipeline reports every stage it enters; feed that straight to the bar.
  api.setProgressListener((fraction, code) => {
    app.progress = code === 'done' ? null : { fraction, code };
  });

  $effect(() => {
    // Fetch the wasm module and the lens database while the drop zone is still
    // on screen, so the first render does not pay for them.
    api.preload();
  });

  async function pick(file) {
    app.busy = true;
    app.busyKey = 'busy.decode';
    app.busyParams = {};
    app.message = t('upload.decoding', { file: file.name });
    try {
      const info = await api.upload(file, (fraction) => {
        const percent = Math.round(fraction * 100);
        const done = percent >= 100;
        app.message = done
          ? t('upload.decoding', { file: file.name })
          : t('upload.sendingPct', { file: file.name, percent });
        app.busyKey = done ? 'busy.decode' : 'busy.upload';
        app.busyParams = done ? {} : { percent };
      });
      // Only once the new file is safely decoded: a rejected upload leaves the
      // picture you were working on exactly as it was.
      resetForNewPhoto();
      app.id = info.id;
      app.info = info;
      app.message = '';
      app.step = FRAME;
    } catch (err) {
      app.message = errorText(err);
    } finally {
      app.busy = false;
      app.busyKey = '';
      app.progress = null;
    }
  }

  /*
   * ── rendering ────────────────────────────────────────────────────────── One
   * effect watches the settings and the current step.
   */

  let token = 0;

  const LABELS = {
    [FRAME]: 'busy.framing',
    [EXPOSURE]: 'busy.preview',
    [WB]: 'busy.preview',
    [VIBRANCE]: 'busy.preview',
    [COMPARE]: 'busy.compare',
    [LOOKS]: 'busy.looks',
    [DOWNLOAD]: 'busy.preview',
  };

  async function refresh(step, key) {
    const mine = ++token;
    const done = (fn) => { if (mine === token) fn(); };
    app.busy = true;
    app.busyKey = LABELS[step] ?? 'busy.working';
    app.busyParams = {};
    try {
      if (step === FRAME && app.frame.key !== key) {
        const out = await api.render({ id: app.id, settings: app.settings,
                                       uncropped: true, size: 1100 });
        done(() => {
          // The angle travels with the image.
          app.frame = { key, image: out.image, angle: app.settings.framing.angle };
          app.coverCrop = out.cover_crop;
          app.toggles = out.toggles;
        });
      } else if ([EXPOSURE, WB, VIBRANCE].includes(step) && app.preview.key !== key) {
        const out = await api.render({ id: app.id, settings: app.settings, size: 1300 });
        done(() => {
          app.preview = { key, image: out.image };
          app.toggles = out.toggles;
        });
      } else if (step === COMPARE && app.wipe.key !== key) {
        const out = await api.compare({ id: app.id, settings: app.settings, size: 1300 });
        done(() => {
          app.wipe = { key, before: out.before, after: out.after };
          app.toggles = out.toggles;
        });
      } else if (step === LOOKS && app.grid.key !== key) {
        // Tiles arrive one at a time, so the grid fills in instead of waiting
        // for all ten looks to finish.
        const out = await api.styles({ id: app.id, settings: app.settings, size: 1200 },
          (tile) => done(() => {
            const tiles = app.grid.key === key ? [...app.grid.tiles, tile] : [tile];
            app.grid = { key, tiles };
          }));
        done(() => (app.grid = { key, tiles: out.tiles }));
      } else if (step === DOWNLOAD) {
        const style = app.chosen[0] || 'original';
        if (app.output.key === `${key}|${style}`) return;
        const out = await api.render({ id: app.id, settings: app.settings, style, size: 1300 });
        done(() => {
          app.output = { key: `${key}|${style}`, image: out.image };
          app.toggles = out.toggles;
        });
      }
    } catch (err) {
      app.message = errorText(err);
    } finally {
      if (mine === token) {
        app.busy = false;
        app.busyKey = '';
        app.progress = null;
      }
    }
  }

  $effect(() => {
    const step = app.step;
    const key = settingsKey();
    const style = app.chosen[0];          // only the download preview cares
    if (!app.id) return;
    const timer = setTimeout(() => refresh(step, key), 200);
    return () => clearTimeout(timer);
  });

  const go = (step) => (app.step = Math.min(Math.max(step, 0), STEPS.length - 1));
</script>

<div class="shell">
  <header>
    <div class="brand">autoraw<span>{t('app.tagline')}</span></div>
    <Stepper onstep={go} />
    <LocalePicker />
  </header>
  <Progress />

  <main>
    {#if app.step === UPLOAD}
      <div class="intro">
        <Privacy />
        <Dropzone onpick={pick} />
      </div>
    {:else if app.step === FRAME}
      <Framing />
    {:else if app.step === EXPOSURE}
      <Brightness />
    {:else if app.step === WB}
      <WhiteBalance />
    {:else if app.step === VIBRANCE}
      <Vibrance />
    {:else if app.step === COMPARE}
      <BeforeAfter />
    {:else if app.step === LOOKS}
      <Looks />
    {:else if app.step === DOWNLOAD}
      <Download />
    {/if}
  </main>

  <footer>
    <button class="ghost" disabled={app.step === UPLOAD} onclick={() => go(app.step - 1)}>
      {t('nav.back')}
    </button>
    <span class="file-label">{fileLabel}</span>
    <button disabled={!app.id || app.step === STEPS.length - 1} onclick={() => go(app.step + 1)}>
      {app.step === STEPS.length - 2 ? t('step.download') : t('nav.continue')}
    </button>
  </footer>
</div>

<style>
  /*
   * The privacy notice sits above the drop zone, which then takes whatever
   * height is left rather than insisting on a full page of its own.
   */
  .intro {
    display: flex; flex-direction: column; height: 100%;
    max-width: 1080px; margin-inline: auto;
  }
  .intro :global(.drop) { flex: 1; height: auto; min-height: 260px; }
</style>
