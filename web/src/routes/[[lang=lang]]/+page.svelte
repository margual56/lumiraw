<script>
  import Stepper from '$lib/components/Stepper.svelte';
  import Progress from '$lib/components/Progress.svelte';
  import Dropzone from '$lib/components/Dropzone.svelte';
  import GradeCarry from '$lib/components/GradeCarry.svelte';
  import Filmstrip from '$lib/components/Filmstrip.svelte';
  import { pickMany } from '$lib/roll.svelte.js';
  import { onMount } from 'svelte';
  import { embedded, listen } from '$lib/demo.js';
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

  import SiteHeader from '$lib/components/SiteHeader.svelte';
  import { ELSEWHERE, READING } from '$lib/pages.js';
  import * as api from '$lib/api.js';
  import { t, localized, address, i18n } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { NAME, SITE } from '$lib/brand.js';
  import Meta from '$lib/components/Meta.svelte';
  import { history, observe, resetHistory, undo, redo, onKey } from '$lib/history.svelte.js';
  import { remember, forget } from '$lib/memory.js';
  import { latest } from '$lib/latest.js';
  import {
    app, settingsKey, isBlank, stepVisible, setStraight, ungraded, defaultSettings,
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
  let saving;
  $effect(() => {
    const key = settingsKey();
    if (app.id !== historyFor) {
      historyFor = app.id;
      resetHistory();
    } else if (key) {
      observe();
      const file = app.fileKey;
      const settings = $state.snapshot(app.settings);
      const blank = isBlank(key);
      clearTimeout(saving);
      saving = setTimeout(() => (blank ? forget(file) : remember(file, settings)), 500);
    }
  });

  /** Throw away a remembered edit and start from what the pipeline decides.
   *  One step in the history like any other, so it can be undone. */
  function startOver() {
    app.settings = defaultSettings();
    app.restored = false;
  }

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

  // Fetch the wasm module and the lens database while the drop zone is still
  // on screen, so the first render does not pay for them. A failure here
  // surfaces again when a file is opened.
  $effect(() => {
    api.ready()
      .then(async (info) => {
        if (info?.version && info.version !== app.version) {
          console.warn(`${NAME}: page is ${app.version} but the pipeline is `
                       + `${info.version}; a stale cached module is in play`);
        }
        // The named grades live in the wasm, so there is only one copy of them.
        app.looks = (await api.looks()).looks ?? [];
      })
      .catch(() => {});
  });

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
        const out = await api.render({ settings: app.settings, uncropped: true, size: 1100 });
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
        const live = app.live;
        const out = await api.render({ settings: app.settings, size: 1300, live });
        done(() => {
          // A frame made while dragging is a quick one, and is made again
          // properly once the control is let go.
          app.preview = { key: live ? `${key}|live` : key, image: out.image };
          app.focus = out.report?.focus ?? app.focus;
          app.toggles = out.toggles;
        });
      }
      // The grade step's "before", keyed on the settings without the grade,
      // so moving a grade control never develops it again.
      const bare = ungraded(app.settings);
      const bareKey = `${app.id}|${JSON.stringify(bare)}`;
      if (step === LOOKS && app.ungraded.key !== bareKey) {
        const out = await api.render({ settings: bare, size: 1300 });
        done(() => { app.ungraded = { key: bareKey, image: out.image }; });
      }
      if (step === COMPARE && app.wipe.key !== key) {
        const out = await api.compare({ settings: app.settings, size: 1300 });
        done(() => {
          app.wipe = { key, before: out.before, after: out.after };
          app.toggles = out.toggles;
        });
      } else if (step === DOWNLOAD && app.output.key !== key) {
        const out = await api.render({ settings: app.settings, size: 1300 });
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

  const schedule = latest(refresh);

  $effect(() => {
    const step = app.step;
    const key = settingsKey();
    if (!app.id) return;
    // Settled controls wait for a pause; a dragged one goes straight away.
    const timer = setTimeout(() => schedule(step, key), app.live ? 0 : 200);
    return () => clearTimeout(timer);
  });

  // Skin is read off this photograph's faces once, the first time a step that
  // moves colour is opened, and kept with its settings from then on.
  let skinFor = null;
  $effect(() => {
    if (!app.id || ![VIBRANCE, LOOKS].includes(app.step) || app.settings.skin !== null) return;
    if (skinFor === app.id) return;
    const id = (skinFor = app.id);
    api.skin($state.snapshot(app.settings))
      .then((found) => { if (app.id === id) app.settings.skin = found; })
      .catch(() => { if (app.id === id) app.settings.skin = { faces: 0 }; });
  });

  // The curve editor's plot, while the editor is on screen. Dragging a point
  // asks on every move, so only the latest request waits behind the one out.
  const bakeCurves = latest(async (curves) => {
    try {
      app.curve = await api.curves({ settings: { curves } });
    } catch {
      // The plot keeps its last shape; the render reports the error.
    }
  });
  $effect(() => {
    const curves = $state.snapshot(app.settings.curves);
    if (app.id && app.step === LOOKS) bakeCurves(curves);
  });

  // Inside a blog post, the post sends the photo and picks the step
  // (lib/demo.js): no header with the steps, and no Back or Continue.
  let embed = $state(false);
  onMount(() => {
    if (!embedded()) return;
    embed = true;
    return listen();
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
        url: SITE + address('/'),
        inLanguage: i18n.locale,
        description: t('seo.home.description'),
        applicationCategory: 'MultimediaApplication',
        operatingSystem: 'Any',
        browserRequirements: 'Requires WebAssembly',
        offers: { '@type': 'Offer', price: '0', priceCurrency: 'EUR' },
      }} />

<div class="shell">
  {#if !embed}
    <SiteHeader tagline={t('app.tagline')} home compact>
      <Stepper onstep={go} />
    </SiteHeader>
  {/if}
  <Progress />

  <main>
    {#if app.restored && app.step !== UPLOAD && !embed}
      <div class="restored">
        <span>{t('memory.restored')}</span>
        <button class="ghost" onclick={startOver}>{t('memory.startOver')}</button>
        <button class="ghost icon" aria-label={t('memory.dismiss')}
                onclick={() => (app.restored = false)}>✕</button>
      </div>
    {/if}
    {#if app.step === UPLOAD && embed}
      <p class="waiting">{app.message || t('demo.waiting')}</p>
    {:else if app.step === UPLOAD}
      <div class="intro">
        <GradeCarry />
        <label class="straight">
          <input type="checkbox" checked={app.straight}
                 onchange={(e) => setStraight(e.currentTarget.checked)} />
          <span>{t('upload.straight')}</span>
        </label>
        <Dropzone onpick={pickMany} />
        <Privacy />
        <a class="merge-link sky-link" href={localized('/sky')}>
          <strong>{t('sky.link')}</strong>
          <span>{t('sky.linkHint')}</span>
        </a>
        <a class="merge-link cull-link" href={localized('/merge')}>
          <strong>{t('merge.link')}</strong>
          <span>{t('merge.linkHint')}</span>
        </a>
        <a class="merge-link cull-link" href={localized('/cull')}>
          <strong>{t('cull.link')}</strong>
          <span>{t('cull.linkHint')}</span>
        </a>
        <a class="merge-link cull-link" href={localized('/calculators')}>
          <strong>{t('calc.link')}</strong>
          <span>{t('calc.linkHint')}</span>
        </a>
        <nav class="more">
          {#each READING as link (link.path)}
            <a href={localized(link.path)}>{t(link.key)}</a>
          {/each}
          {#each ELSEWHERE as link (link.key)}
            <a href={link.href[i18n.locale] ?? link.href.en}>{t(link.key)}</a>
          {/each}
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

  {#if !embed}
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
  {/if}
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;
  @use '../../styles/surfaces' as *;

  /*
   * The privacy notice sits above the drop zone, which then takes whatever
   * height is left rather than insisting on a full page of its own.
   */
  .waiting { display: grid; place-items: center; height: 100%; margin: 0; color: var(--color-muted); text-align: center; padding: 24px; }

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

  /* Said once, when a photograph opens with an earlier edit already on it. */
  .restored {
    @include surface;
    display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px;
    max-width: 1080px; margin: 0 auto 14px; padding: 8px 8px 8px 14px;
    font-size: 13px; color: var(--color-muted);
    span { flex: 1; min-width: 12em; }
    button { padding: 6px 12px; }
    .icon { padding-inline: 10px; }
    @include phone { font-size: 14px; }
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
  /* The ones after the first, close under it. */
  .cull-link { margin-top: 8px; }
  /* The sky planner, as a night sky: stars of all brightnesses over deep blue, thicker along a band. */
  .sky-link {
    border-color: #2b3c78;
    background:
      radial-gradient(1.2px 1.2px at 14% 37%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1px 1px at 27% 11%, rgba(205,218,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 28% 97%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 18% 19%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 9% 33%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 3% 21%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.6px 1.6px at 97% 66%, rgba(255,236,214,0.9) 60%, transparent),
      radial-gradient(1px 1px at 64% 50%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 38% 44%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1.6px 1.6px at 94% 87%, rgba(205,218,255,0.9) 60%, transparent),
      radial-gradient(1px 1px at 39% 48%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 85% 21%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1px 1px at 54% 63%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(2.4px 2.4px at 79% 73%, rgba(255,236,214,1) 60%, transparent),
      radial-gradient(1px 1px at 83% 9%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 23% 28%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 8% 42%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 1% 3%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 65% 68%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 14% 39%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 86% 42%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 21% 28%, rgba(205,218,255,0.45) 60%, transparent),
      radial-gradient(1.6px 1.6px at 90% 79%, rgba(255,255,255,0.9) 60%, transparent),
      radial-gradient(1px 1px at 21% 39%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 39% 43%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1.6px 1.6px at 61% 38%, rgba(205,218,255,0.9) 60%, transparent),
      radial-gradient(1.2px 1.2px at 74% 63%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1.6px 1.6px at 51% 19%, rgba(205,218,255,0.9) 60%, transparent),
      radial-gradient(1.6px 1.6px at 94% 29%, rgba(255,255,255,0.9) 60%, transparent),
      radial-gradient(1px 1px at 93% 86%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1.6px 1.6px at 75% 31%, rgba(255,236,214,0.9) 60%, transparent),
      radial-gradient(1.2px 1.2px at 92% 48%, rgba(205,218,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 37% 37%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1px 1px at 32% 25%, rgba(205,218,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 18% 36%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 48% 39%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 55% 50%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 15% 56%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1px 1px at 46% 44%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1px 1px at 57% 57%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 57% 62%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1px 1px at 79% 12%, rgba(205,218,255,0.45) 60%, transparent),
      radial-gradient(1.6px 1.6px at 75% 22%, rgba(255,255,255,0.9) 60%, transparent),
      radial-gradient(1.2px 1.2px at 64% 80%, rgba(205,218,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 56% 90%, rgba(255,236,214,0.45) 60%, transparent),
      radial-gradient(1px 1px at 56% 57%, rgba(205,218,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 41% 49%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(2.4px 2.4px at 77% 66%, rgba(205,218,255,1) 60%, transparent),
      radial-gradient(1.2px 1.2px at 84% 86%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1px 1px at 96% 81%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 91% 36%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1px 1px at 65% 61%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 77% 62%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 70% 46%, rgba(255,236,214,0.7) 60%, transparent),
      radial-gradient(1.6px 1.6px at 58% 52%, rgba(255,236,214,0.9) 60%, transparent),
      radial-gradient(1.2px 1.2px at 73% 33%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.2px 1.2px at 92% 62%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(1.6px 1.6px at 99% 33%, rgba(255,236,214,0.9) 60%, transparent),
      radial-gradient(2.4px 2.4px at 55% 70%, rgba(205,218,255,1) 60%, transparent),
      radial-gradient(1px 1px at 17% 62%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 33% 59%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 95% 5%, rgba(255,255,255,0.7) 60%, transparent),
      radial-gradient(2.4px 2.4px at 83% 60%, rgba(205,218,255,1) 60%, transparent),
      radial-gradient(1px 1px at 24% 23%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1px 1px at 56% 46%, rgba(255,255,255,0.45) 60%, transparent),
      radial-gradient(1.2px 1.2px at 10% 52%, rgba(255,255,255,0.7) 60%, transparent),
      linear-gradient(135deg, #0b1230, #1d2b5e);
    /* The stars fade behind the words, or they read as punctuation. */
    strong, span { text-shadow: 0 0 3px #0b1230, 0 0 6px #0b1230, 0 0 10px #0b1230; }
    strong { color: #f2c46a; }
    span { color: rgba(255, 255, 255, .78); }
    &:hover { border-color: #f2c46a; background-color: transparent; filter: brightness(1.12); }
  }
</style>
