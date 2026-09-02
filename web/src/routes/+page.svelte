<script>
  import Stepper from '$lib/components/Stepper.svelte';
  import Progress from '$lib/components/Progress.svelte';
  import Dropzone from '$lib/components/Dropzone.svelte';
  import GradeCarry from '$lib/components/GradeCarry.svelte';
  import Filmstrip from '$lib/components/Filmstrip.svelte';
  import { pickMany } from '$lib/roll.svelte.js';
  import Privacy from '$lib/components/Privacy.svelte';
  import Framing from '$lib/steps/Framing.svelte';
  import Refocus from '$lib/steps/Refocus.svelte';
  import Brightness from '$lib/steps/Brightness.svelte';
  import WhiteBalance from '$lib/steps/WhiteBalance.svelte';
  import Vibrance from '$lib/steps/Vibrance.svelte';
  import Local from '$lib/steps/Local.svelte';
  import BeforeAfter from '$lib/steps/BeforeAfter.svelte';
  import Looks from '$lib/steps/Looks.svelte';
  import Download from '$lib/steps/Download.svelte';

  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import { base } from '$app/paths';
  import * as api from '$lib/api.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { NAME, SITE } from '$lib/brand.js';
  import Meta from '$lib/components/Meta.svelte';
  import { history, observe, resetHistory, undo, redo, onKey } from '$lib/history.svelte.js';
  import {
    app, settingsKey, stepVisible, setStraight, ungraded,
    STEPS, UPLOAD, FRAME, REFOCUS, EXPOSURE, WB, VIBRANCE, LOCAL, COMPARE, LOOKS, DOWNLOAD,
  } from '$lib/state.svelte.js';

  const fileLabel = $derived(app.info
    ? [app.info.original_name,
       `${app.info.width}×${app.info.height}`,
       app.info.profile.camera,
       app.info.profile.lens || t('profile.unknownLens'),
       `ISO ${app.info.profile.iso}`,
       `f/${app.info.profile.aperture}`].join(' · ')
    : '');

  // Undo history is per photograph.
  let historyFor = null;
  $effect(() => {
    const key = settingsKey();
    if (app.id !== historyFor) {
      historyFor = app.id;
      resetHistory();
    } else if (key) {
      observe();
    }
  });

  // Opened through the system's "Open with" on an installed LumiRaw: the
  // files arrive as handles, and become a roll like any other drop.
  $effect(() => {
    if (!('launchQueue' in window)) return;
    window.launchQueue.setConsumer(async ({ files }) => {
      if (!files?.length) return;
      pickMany(await Promise.all(files.map((handle) => handle.getFile())));
    });
  });

  // The pipeline reports every stage it enters; feed that straight to the bar.
  api.setProgressListener((fraction, code) => {
    app.progress = code === 'done' ? null : { fraction, code };
  });

  $effect(() => {
    // Fetch the wasm module and the lens database while the drop zone is still
    // on screen, so the first render does not pay for them.
    api.ready()
      .then(async (info) => {
        if (info?.version && info.version !== app.version) {
          console.warn(`${NAME}: page is ${app.version} but the pipeline is `
                       + `${info.version}; a stale cached module is in play`);
        }
        // The named grades, once. They are the same for every photograph and
        // they come from the wasm so there is only one copy of them.
        app.looks = (await api.looks()).looks ?? [];
      })
      .catch(() => {});
  });



  /*
   * ── rendering ────────────────────────────────────────────────────────── One
   * effect watches the settings and the current step.
   */

  let token = 0;

  const LABELS = {
    [FRAME]: 'busy.framing',
    [REFOCUS]: 'busy.preview',
    [EXPOSURE]: 'busy.preview',
    [WB]: 'busy.preview',
    [VIBRANCE]: 'busy.preview',
    [LOCAL]: 'busy.preview',
    [COMPARE]: 'busy.compare',
    [LOOKS]: 'busy.preview',
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
          app.framingHint = out.report?.framing ?? null;
          app.focus = out.report?.focus ?? null;
          app.toggles = out.toggles;
        });
      } else if ([REFOCUS, EXPOSURE, WB, VIBRANCE, LOCAL, LOOKS].includes(step)
                 && app.preview.key !== key) {
        const out = await api.render({ id: app.id, settings: app.settings, size: 1300 });
        done(() => {
          app.preview = { key, image: out.image };
          app.focus = out.report?.focus ?? app.focus;
          app.toggles = out.toggles;
        });
      }
      // The grade step's "before", keyed on the settings without the grade,
      // so moving a grade control never develops it again.
      const bare = ungraded(app.settings);
      const bareKey = `${app.id}|${JSON.stringify(bare)}`;
      if (step === LOOKS && app.ungraded.key !== bareKey) {
        const out = await api.render({ id: app.id, settings: bare, size: 1300 });
        done(() => { app.ungraded = { key: bareKey, image: out.image }; });
      }
      if (step === COMPARE && app.wipe.key !== key) {
        const out = await api.compare({ id: app.id, settings: app.settings, size: 1300 });
        done(() => {
          app.wipe = { key, before: out.before, after: out.after };
          app.toggles = out.toggles;
        });
      } else if (step === DOWNLOAD && app.output.key !== key) {
        const out = await api.render({ id: app.id, settings: app.settings, size: 1300 });
        done(() => {
          app.output = { key, image: out.image };
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
    if (!app.id) return;
    const timer = setTimeout(() => refresh(step, key), 200);
    return () => clearTimeout(timer);
  });

  // The curve plot, kept in step with the settings.
  $effect(() => {
    const curves = JSON.stringify(app.settings.curves);
    if (!app.id) return;
    api.curves({ id: app.id, settings: { curves: JSON.parse(curves) } })
       .then((out) => (app.curve = out))
       .catch(() => {});
  });

  /** Move to a step, stepping over any that this photograph does not need. */
  function go(step) {
    const target = Math.min(Math.max(step, 0), STEPS.length - 1);
    const direction = target >= app.step ? 1 : -1;
    let next = target;
    while (next > 0 && next < STEPS.length - 1 && !stepVisible(next)) {
      next += direction;
    }
    app.step = stepVisible(next) ? next : app.step;
  }
</script>

<svelte:window onkeydown={onKey} />

<Meta title={t('seo.home.title')} description={t('seo.home.description')} path="/"
      schema={{
        '@context': 'https://schema.org',
        '@type': 'WebApplication',
        name: NAME,
        url: `${SITE}/`,
        description: t('seo.home.description'),
        applicationCategory: 'MultimediaApplication',
        operatingSystem: 'Any',
        browserRequirements: 'Requires WebAssembly',
        offers: { '@type': 'Offer', price: '0', priceCurrency: 'EUR' },
      }} />

<div class="shell">
  <header>
    <h1 class="brand">
      {NAME}
      <span>
        <span class="tagline">{t('app.tagline')}</span>
        {#if app.version}<span class="ver">v{app.version}</span>{/if}
      </span>
    </h1>
    <Stepper onstep={go} />
    <LocalePicker />
  </header>
  <Progress />

  <main>
    {#if app.step === UPLOAD}
      <div class="intro">
        <Privacy />
        <GradeCarry />
        <label class="straight">
          <input type="checkbox" checked={app.straight}
                 onchange={(e) => setStraight(e.currentTarget.checked)} />
          <span>{t('upload.straight')}</span>
        </label>
        <Dropzone onpick={pickMany} />
        <a class="merge-link" href="{base}/merge">
          <strong>{t('merge.link')}</strong>
          <span>{t('merge.linkHint')}</span>
        </a>
        <nav class="more">
          <a href="{base}/about">{t('links.about')}</a>
          <a href="{base}/cameras">{t('links.cameras')}</a>
        </nav>
      </div>
    {:else if app.step === FRAME}
      <Framing />
    {:else if app.step === REFOCUS}
      <Refocus />
    {:else if app.step === EXPOSURE}
      <Brightness />
    {:else if app.step === WB}
      <WhiteBalance />
    {:else if app.step === VIBRANCE}
      <Vibrance />
    {:else if app.step === LOCAL}
      <Local />
    {:else if app.step === COMPARE}
      <BeforeAfter />
    {:else if app.step === LOOKS}
      <Looks />
    {:else if app.step === DOWNLOAD}
      <Download />
    {/if}
  </main>

  <Filmstrip />

  <footer class:idle={!app.id}>
    <span class="nav-start">
      <button class="ghost" disabled={app.step === UPLOAD} onclick={() => go(app.step - 1)}>
        {t('nav.back')}
      </button>
      {#if app.id && app.step !== UPLOAD}
        <button class="ghost icon" disabled={!history.canUndo} onclick={undo}
                title={t('nav.undo.key')} aria-label={t('nav.undo')}>↶</button>
        <button class="ghost icon" disabled={!history.canRedo} onclick={redo}
                title={t('nav.redo.key')} aria-label={t('nav.redo')}>↷</button>
      {/if}
    </span>
    <span class="file-label">{fileLabel}</span>
    <span class="nav-end">
    {#if app.id && app.step < DOWNLOAD - 1}
      <button class="ghost skip" onclick={() => go(DOWNLOAD)} title={t('nav.straight.why')}>
        {t('nav.straight')}
      </button>
    {/if}
    <button class="next" class:last={app.step === STEPS.length - 1}
            disabled={!app.id || app.step === STEPS.length - 1} onclick={() => go(app.step + 1)}>
      {app.step === STEPS.length - 2 ? t('step.download') : t('nav.continue')}
    </button>
    </span>
  </footer>
</div>

<style lang="scss">
  @use '../styles/breakpoints' as *;
  @use '../styles/surfaces' as *;

  /*
   * The tagline goes on a phone and the version stays, since the version is the
   * half worth the screen space.
   */
  @include phone {
    .tagline { display: none; }
  }

  /*
   * The privacy notice sits above the drop zone, which then takes whatever
   * height is left rather than insisting on a full page of its own.
   */
  .intro {
    display: flex; flex-direction: column; height: 100%;
    max-width: 1080px; margin-inline: auto;
    :global(.drop) { flex: 1; height: auto; min-height: 260px; }
  }

  /* The way out to the other tool, under the drop zone rather than beside it. */
  .nav-end { display: flex; gap: 8px; }
  .nav-start {
    display: flex; gap: 6px;
    .icon {
      min-width: 36px; padding-inline: 8px; font-size: 16px; line-height: 1;
      @include phone { min-width: 44px; font-size: 18px; }
    }
  }

  /* One row on a phone, with the way forward taking what is left of it. */
  @include phone {
    footer.idle { display: none; }
    .nav-end { flex: 1; }
    .nav-end .next { flex: 1; }
    .skip, .next.last { display: none; }
  }

  /* For whoever wants to know more before handing over a file, and for
     search engines, which only find the other pages through links. */
  .more {
    display: flex; gap: 18px; margin-top: 12px; padding-left: 2px; font-size: 12.5px;
    a { color: var(--color-muted); text-decoration: none; &:hover { color: var(--color-ink); } }
    @include phone {
      gap: 8px; margin-top: 6px; font-size: 14px;
      a { padding: 10px 6px; }
    }
  }

  .straight {
    display: flex; gap: 8px; align-items: center; margin: -4px 0 10px;
    font-size: 12.5px; color: var(--color-muted); cursor: pointer;
    input { accent-color: var(--color-accent); }
  }

  .merge-link {
    @include surface;
    display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px;
    margin-top: 12px; padding: 12px 14px;
    text-decoration: none; color: var(--color-ink);
    &:hover { border-color: var(--color-accent); background: var(--color-panel-2); }
    strong { font-size: 14px; font-weight: 600; color: var(--color-accent); }
    span { color: var(--color-muted); font-size: 13px; }
  }
</style>
