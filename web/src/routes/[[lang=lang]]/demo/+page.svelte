<script>
  /**
   * Four narrated recordings of the app, one screen each, like a feed of
   * reels: scroll, the arrow keys or the counter move between them, and each
   * goes on to the next when it ends. Recorded by tools/demo (see its README).
   */
  import { onMount } from 'svelte';
  import { base } from '$app/paths';
  import Meta from '$lib/components/Meta.svelte';
  import { localized, t } from '$lib/i18n.svelte.js';

  const CLIPS = [
    { id: 'develop', url: 'lumiraw.app' },
    { id: 'cull', url: 'lumiraw.app/cull' },
    { id: 'merge', url: 'lumiraw.app/merge' },
    { id: 'sky', url: 'lumiraw.app/sky' },
  ];
  const reels = [{ kind: 'intro', id: 'intro' }, ...CLIPS.map((c) => ({ kind: 'clip', ...c })), { kind: 'end', id: 'end' }];
  const media = (file) => `${base}/videos/${file}`;

  let active = $state(0);
  let progress = $state(0);
  let auto = $state(true);
  let sound = $state(false);
  let still = $state(false);
  let scroller;
  const sections = [];
  const videos = [];
  let timer;

  /** Plays the reel in view and only that one, from the start. */
  function sync() {
    videos.forEach((video, i) => {
      if (!video) return;
      // Captions while it is silent; the voice says the same with sound on.
      if (video.textTracks[0]) video.textTracks[0].mode = sound ? 'hidden' : 'showing';
      if (i !== active || still) { video.pause(); return; }
      video.muted = !sound;
      video.play().catch(() => {
        // Sound needs a click first; carry on without it.
        if (!video.muted) { sound = false; video.muted = true; video.play().catch(() => {}); }
      });
    });
  }

  function enter(i) {
    if (i < 0 || i === active) return;
    active = i;
    progress = 0;
    const video = videos[i];
    if (video) video.currentTime = 0;
    sync();
    wait();
  }

  /** The opening screen moves on by itself after a while; the last one stays. */
  function wait() {
    clearInterval(timer);
    if (!auto || still || reels[active].kind !== 'intro') return;
    const from = Date.now(), ms = 6000;
    timer = setInterval(() => {
      progress = Math.min(1, (Date.now() - from) / ms);
      if (progress === 1) { clearInterval(timer); go(active + 1); }
    }, 100);
  }

  function go(i) {
    sections[Math.max(0, Math.min(reels.length - 1, i))]?.scrollIntoView({ behavior: still ? 'auto' : 'smooth' });
  }

  function watch(withSound) {
    sound = withSound;
    go(1);
  }

  function toggleSound() {
    sound = !sound;
    sync();
  }

  function toggleAuto() {
    auto = !auto;
    progress = 0;
    wait();
  }

  function onKey(e) {
    if (e.target.closest('input, textarea, select')) return;
    const onControl = Boolean(e.target.closest('button, a'));
    const to = { ArrowDown: 1, PageDown: 1, j: 1, ArrowUp: -1, PageUp: -1, k: -1 }[e.key]
      ?? (e.key === ' ' && !onControl ? 1 : null);
    if (to !== null) { e.preventDefault(); go(active + to); }
    else if (e.key === 'Home') { e.preventDefault(); go(0); }
    else if (e.key === 'End') { e.preventDefault(); go(reels.length - 1); }
    else if (e.key === 'm') toggleSound();
  }

  function ended(i) {
    if (i === active && auto) go(active + 1);
  }

  onMount(() => {
    // Nothing plays or moves on by itself for someone who asked for less motion.
    still = matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (still) auto = false;
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) if (entry.isIntersecting) enter(sections.indexOf(entry.target));
    }, { root: scroller, threshold: 0.6 });
    sections.forEach((s) => observer.observe(s));
    sync();
    wait();
    return () => { observer.disconnect(); clearInterval(timer); };
  });

  const fill = (i) => (i < active ? 1 : i > active ? 0 : reels[i].kind === 'end' ? 1 : progress);
</script>

<Meta title={t('demo.seo.title')} description={t('demo.seo.description')} path="/demo" index={false} />

<svelte:window onkeydown={onKey} />

<div class="demo">
  <div class="progress" aria-hidden="true">
    {#each reels as reel, i (reel.id)}
      <span><i style:transform="scaleX({fill(i)})"></i></span>
    {/each}
  </div>

  <a class="close" href={localized('/')}>
    <svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20"><path fill="currentColor"
      d="m6.4 19-1.4-1.4 5.6-5.6L5 6.4 6.4 5l5.6 5.6L17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6Z" /></svg>
    <span>{t('demo.close')}</span>
  </a>

  <div class="rail" role="group" aria-label={t('demo.controls')}>
    <button type="button" class="rail-button" aria-pressed={sound} onclick={toggleSound}>
      <svg aria-hidden="true" viewBox="0 0 24 24" width="26" height="26">
        {#if sound}
          <path fill="currentColor" d="M14 20.7v-2.1q2.2-.6 3.6-2.5T19 12q0-2.2-1.4-4.1T14 5.4V3.3q3.1.7 5 3.1T21 12q0 3.2-2 5.6t-5 3.1ZM3 15V9h4l5-5v16l-5-5H3Zm11 1V8q1.2.5 1.8 1.6T16.5 12q0 1.3-.7 2.4T14 16Z" />
        {:else}
          <path fill="currentColor" d="m19.8 22.6-3-3q-.6.4-1.3.7t-1.5.5v-2.1l.6-.2.6-.3L12 14.8V20l-5-5H3V9h3.2L1.4 4.2l1.4-1.4 18.4 18.4-1.4 1.4Zm-.2-5.8-1.4-1.4q.4-.8.6-1.6t.2-1.8q0-2.4-1.4-4.2T14 5.4V3.3q3.1.7 5 3.2t2 5.5q0 1.3-.4 2.6t-1 2.2Zm-3.4-3.4L14 11.2V7.9q1.2.6 1.8 1.7t.6 2.4q0 .4-.1.7t-.1.7ZM12 9.2 9.4 6.6 12 4v5.2Z" />
        {/if}
      </svg>
      <span class="rail-label">{sound ? t('demo.soundOn') : t('demo.soundOff')}</span>
    </button>
    <button type="button" class="rail-button" aria-pressed={auto} onclick={toggleAuto}>
      <svg aria-hidden="true" viewBox="0 0 24 24" width="26" height="26"><path fill="currentColor"
        d="M12 22q-2.1 0-3.9-.8t-3.2-2.1q-1.4-1.4-2.1-3.2T2 12q0-2.1.8-3.9t2.1-3.2Q6.3 3.6 8.1 2.8T12 2q2.3 0 4.3 1T19.8 5.8V3h2v6h-6V7h2.8q-1-1.4-2.6-2.2T12 4Q8.7 4 6.3 6.3T4 12q0 3.3 2.3 5.7T12 20q3 0 5.2-1.9t2.7-4.9l2 .3q-.5 3.8-3.3 6.2T12 22Zm2.8-5.8L11 12.4V7h2v4.6l3.2 3.2-1.4 1.4Z" /></svg>
      <span class="rail-label">{t('demo.auto')}</span>
    </button>
    <p class="counter">{t('demo.counter', { n: active + 1, total: reels.length })}</p>
  </div>

  <main class="feed" bind:this={scroller} aria-label={t('demo.label')}>
    {#each reels as reel, i (reel.id)}
      <section class="reel {reel.kind}" class:active={i === active} bind:this={sections[i]}
               aria-labelledby="t-{reel.id}">
        {#if reel.kind === 'intro'}
          <div class="body">
            <p class="wordmark">Lumi<span>Raw</span></p>
            <h1 id="t-intro" class="title">{t('demo.intro.title')}</h1>
            <p class="text">{t('demo.intro.text')}</p>
            <p class="actions">
              <button type="button" class="cta primary" onclick={() => watch(true)}>{t('demo.watch')}</button>
              <button type="button" class="cta" onclick={() => watch(false)}>{t('demo.watchMuted')}</button>
            </p>
          </div>
        {:else if reel.kind === 'clip'}
          <div class="copy">
            <p class="eyebrow">{t(`demo.${reel.id}.eyebrow`)}</p>
            <h2 id="t-{reel.id}" class="title">{t(`demo.${reel.id}.title`)}</h2>
            <p class="text">{t(`demo.${reel.id}.text`)}</p>
          </div>
          <figure class="device">
            <div class="chrome" aria-hidden="true">
              <span class="dots"><i></i><i></i><i></i></span>
              <span class="url">{reel.url}</span>
            </div>
            <video bind:this={videos[i]} src={media(`${reel.id}.mp4`)} poster={media(`${reel.id}.jpg`)}
                   playsinline muted loop={!auto} controls={still}
                   preload={Math.abs(i - active) <= 1 ? 'auto' : 'metadata'}
                   aria-label={t(`demo.${reel.id}.title`)}
                   ontimeupdate={(e) => {
                     const v = e.currentTarget;
                     if (i === active) progress = v.duration ? v.currentTime / v.duration : 0;
                   }}
                   onended={() => ended(i)}>
              <track kind="captions" srclang="en" label="English" src={media(`${reel.id}.vtt`)} default />
            </video>
          </figure>
        {:else}
          <div class="body">
            <h2 id="t-end" class="title">{t('demo.end.title')}</h2>
            <p class="text">{t('demo.end.text')}</p>
            <p class="actions">
              <a class="cta primary" href={localized('/')}>{t('demo.open')}</a>
              <a class="cta" href={localized('/guide')}>{t('demo.guide')}</a>
              <button type="button" class="cta ghost" onclick={() => go(0)}>{t('demo.again')}</button>
            </p>
          </div>
        {/if}
      </section>
    {/each}
  </main>

  <p class="hint" aria-hidden="true">{t('demo.hint')}</p>
</div>

<style lang="scss">
  :global(body:has(.demo)) { overflow: hidden; }
  .demo {
    position: fixed; inset: 0; color: var(--color-ink);
    background:
      radial-gradient(60rem 40rem at 85% 10%, rgb(229 160 60 / .14), transparent 70%),
      radial-gradient(50rem 35rem at 0% 100%, rgb(70 110 180 / .14), transparent 70%),
      var(--color-bg);
  }
  .feed {
    height: 100%; overflow-y: auto; scroll-snap-type: y mandatory;
    overscroll-behavior: contain; scrollbar-width: none;
    &::-webkit-scrollbar { display: none; }
  }
  .reel {
    position: relative; display: grid; align-items: center; overflow: hidden;
    grid-template-columns: minmax(0, .7fr) minmax(0, 1.75fr); gap: clamp(24px, 3.5vw, 56px);
    height: 100dvh; box-sizing: border-box;
    padding: 64px clamp(96px, 8vw, 136px) 48px clamp(16px, 4.5vw, 88px);
    scroll-snap-align: start; scroll-snap-stop: always;
    &.intro, &.end { grid-template-columns: minmax(0, 1fr); justify-items: center; text-align: center; }
  }

  .eyebrow {
    display: inline-block; margin: 0 0 14px; padding: 4px 12px; border-radius: 999px;
    background: var(--color-accent); color: var(--color-accent-ink);
    font-weight: 700; font-size: 13px; letter-spacing: .02em;
  }
  .title {
    margin: 0; font-size: clamp(30px, 3.6vw, 54px); font-weight: 800; line-height: 1.08;
    letter-spacing: -.02em; text-wrap: balance;
  }
  .text {
    max-width: 30em; margin: 18px 0 0; color: var(--color-muted);
    font-size: clamp(15px, 1.3vw, 19px); line-height: 1.5; text-wrap: pretty;
  }
  .intro .text, .end .text { margin-inline: auto; }
  .wordmark {
    margin: 0 0 22px; font-size: clamp(44px, 6vw, 84px); font-weight: 800; letter-spacing: -.03em;
    span { color: var(--color-accent); }
  }
  .intro .title, .end .title { font-size: clamp(32px, 4.6vw, 64px); }
  .body { max-width: 860px; }

  .actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 10px; margin-top: 30px; }
  .cta {
    display: inline-flex; align-items: center; min-height: 46px; padding: 6px 22px; cursor: pointer;
    border: 1px solid var(--color-line); border-radius: 999px; background: var(--color-panel);
    color: var(--color-ink); font: inherit; font-size: 15px; font-weight: 700; text-decoration: none;
    &:hover { border-color: var(--color-muted); }
    &.primary { border-color: var(--color-accent); background: var(--color-accent); color: var(--color-accent-ink); }
    &.ghost { border-color: transparent; background: none; text-decoration: underline; }
  }

  /* Each screen's words and recording come in as it becomes the current one. */
  .copy > *, .body > * {
    opacity: 0; transform: translateY(20px);
    transition: opacity .5s ease, transform .6s cubic-bezier(.2, .8, .2, 1);
  }
  .active :is(.copy, .body) > * { opacity: 1; transform: none; }
  @for $n from 2 through 4 {
    .active :is(.copy, .body) > :nth-child(#{$n}) { transition-delay: ($n - 1) * .08s; }
  }
  .device {
    opacity: 0; transform: translateY(28px) scale(.96);
    transition: opacity .5s ease .1s, transform .7s cubic-bezier(.2, .8, .2, 1) .1s;
  }
  .active .device { opacity: 1; transform: none; }

  .device {
    position: relative; margin: 0; justify-self: center; width: 100%; overflow: hidden;
    border-radius: 12px; background: var(--color-panel);
    box-shadow: 0 30px 80px rgb(0 0 0 / .55), 0 0 0 1px rgb(255 255 255 / .08);
    video {
      display: block; width: 100%; aspect-ratio: 16 / 9; max-height: calc(100dvh - 150px);
      object-fit: contain; background: #000;
    }
    video::cue { font-size: 1.1em; background: rgb(0 0 0 / .72); }
  }
  .chrome {
    display: flex; align-items: center; gap: 14px; height: 32px; padding: 0 12px;
    background: var(--color-panel-2);
  }
  .dots {
    display: flex; gap: 6px;
    i { width: 10px; height: 10px; border-radius: 50%; background: #ff5f57; }
    i:nth-child(2) { background: #febc2e; }
    i:nth-child(3) { background: #28c840; }
  }
  .url {
    flex: 1; max-width: 400px; margin-inline: auto; padding: 2px 12px; border-radius: 999px;
    background: var(--color-bg); color: var(--color-muted); font-size: 12px; text-align: center;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .progress {
    position: fixed; z-index: 5; top: 10px; left: clamp(16px, 4.5vw, 88px); right: clamp(16px, 4.5vw, 88px);
    display: flex; gap: 4px; pointer-events: none;
    span { flex: 1; height: 3px; border-radius: 2px; background: rgb(255 255 255 / .18); overflow: hidden; }
    i { display: block; height: 100%; background: var(--color-ink); transform-origin: left; transition: transform .25s linear; }
  }
  .close {
    position: fixed; z-index: 5; top: 22px; left: clamp(16px, 4.5vw, 88px);
    display: inline-flex; align-items: center; gap: 6px; min-height: 38px; padding: 2px 14px 2px 10px;
    border-radius: 999px; background: rgb(255 255 255 / .07); color: var(--color-ink);
    font-weight: 700; text-decoration: none;
    &:hover { background: rgb(255 255 255 / .14); }
  }
  .rail {
    position: fixed; z-index: 5; right: clamp(8px, 1.6vw, 28px); top: 50%; transform: translateY(-50%);
    display: flex; flex-direction: column; align-items: center; gap: 16px;
  }
  .rail-button {
    display: flex; flex-direction: column; align-items: center; gap: 4px; width: 72px; padding: 0;
    border: 0; background: none; color: var(--color-ink); font: inherit; font-size: 11.5px; font-weight: 700; cursor: pointer;
    svg {
      box-sizing: content-box; padding: 10px; border-radius: 50%; background: rgb(255 255 255 / .1);
      transition: transform .15s, background .15s, color .15s;
    }
    &:hover svg { background: rgb(255 255 255 / .2); }
    &:active svg { transform: scale(.88); }
    &[aria-pressed='true'] svg { background: var(--color-ink); color: var(--color-bg); }
  }
  .rail-label { text-align: center; line-height: 1.2; }
  .counter { margin: 0; color: var(--color-muted); font-weight: 700; font-variant-numeric: tabular-nums; }
  .hint {
    position: fixed; bottom: 10px; left: 50%; transform: translateX(-50%); margin: 0;
    color: rgb(255 255 255 / .45); font-size: 12px; white-space: nowrap; pointer-events: none;
  }
  a:focus-visible, button:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }

  /* Phones and tall windows: the recording first, its words under it. */
  @media (max-width: 760px), (max-aspect-ratio: 4 / 5) {
    .reel {
      grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) auto;
      align-items: start; gap: 16px; padding: 72px 16px 96px;
      &.clip .device { grid-row: 1; align-self: center; }
      &.clip .copy { grid-row: 2; }
      &.intro, &.end { align-content: center; grid-template-rows: auto; }
    }
    .title { font-size: clamp(24px, 7vw, 34px); }
    .text { margin-top: 8px; font-size: 15px; }
    .eyebrow { margin-bottom: 8px; }
    .chrome { display: none; }
    .rail {
      top: auto; bottom: 16px; right: 16px; left: 16px; transform: none;
      flex-direction: row; justify-content: flex-end; gap: 12px;
    }
    .rail-button { width: auto; flex-direction: row; gap: 8px; }
    .rail-button svg { padding: 8px; }
    .counter { order: -1; margin-right: auto; }
    .close span, .hint { display: none; }
    .close { padding: 2px 8px; }
  }

  @media (prefers-reduced-motion: reduce) {
    .copy > *, .body > *, .device { opacity: 1; transform: none; transition: none; }
  }
</style>
