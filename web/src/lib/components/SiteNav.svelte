<script>
  /** The way between pages, at the right of every header, with the language
   *  last. Wide, the four tools are links of their own and the pages to read
   *  sit under "More"; on a phone, or beside the developer's steps, it all
   *  folds into one menu, the language with it. Either way every page is at
   *  most two clicks off. */
  import { page } from '$app/state';
  import LocalePicker from '$lib/components/LocalePicker.svelte';
  import { localized, t, unlocalized } from '$lib/i18n.svelte.js';
  import { READING, TOOLS } from '$lib/pages.js';

  /** Always the menu, never the links: for a header that is full already. */
  let { compact = false } = $props();

  const here = $derived(unlocalized(page.url.pathname));
  const current = (href) => (here === href ? 'page' : undefined);

  /** Which menu is open: 'more' (wide) or 'all' (folded), or none. */
  let open = $state('');
  let root = $state(null);

  // Closed by a click anywhere else, by Escape, and by going somewhere.
  $effect(() => {
    if (!open) return;
    const away = (e) => { if (!root?.contains(e.target)) open = ''; };
    const key = (e) => { if (e.key === 'Escape') open = ''; };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', away);
      document.removeEventListener('keydown', key);
    };
  });
  $effect(() => {
    void page.url.pathname;
    open = '';
  });
  const toggle = (which) => { open = open === which ? '' : which; };
</script>

<div class="site" class:compact bind:this={root}>
  <nav class="inline" aria-label={t('site.label')}>
    {#each TOOLS as link (link.path)}
      <a href={localized(link.path)} aria-current={current(link.path)}>{t(link.key)}</a>
    {/each}
    <div class="pop">
      <button type="button" class="more" aria-expanded={open === 'more'} aria-controls="site-more"
              class:here={READING.some((l) => l.path === here)} onclick={() => toggle('more')}>
        {t('site.more')}<span class="caret" aria-hidden="true"></span>
      </button>
      {#if open === 'more'}
        <div class="menu" id="site-more">
          {#each READING as link (link.path)}
            <a href={localized(link.path)} aria-current={current(link.path)}>{t(link.key)}</a>
          {/each}
        </div>
      {/if}
    </div>
  </nav>

  <div class="pop folded">
    <button type="button" class="burger" aria-expanded={open === 'all'} aria-controls="site-all"
            aria-label={t('site.menu')} onclick={() => toggle('all')}>
      <span class="lines" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="word">{t('site.menu')}</span>
    </button>
    {#if open === 'all'}
      <nav class="menu wide" id="site-all" aria-label={t('site.label')}>
        <span class="group">{t('site.tools')}</span>
        {#each TOOLS as link (link.path)}
          <a href={localized(link.path)} aria-current={current(link.path)}>{t(link.key)}</a>
        {/each}
        <span class="group">{t('site.reading')}</span>
        {#each READING as link (link.path)}
          <a href={localized(link.path)} aria-current={current(link.path)}>{t(link.key)}</a>
        {/each}
        <span class="group">{t('locale.label')}</span>
        <div class="language"><LocalePicker /></div>
      </nav>
    {/if}
  </div>

  <div class="language bar"><LocalePicker /></div>
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  .site {
    display: flex; align-items: center; gap: 6px; margin-left: auto;
  }

  .inline {
    display: flex; align-items: center; gap: 2px;
    > a, .more {
      padding: 6px 10px; border-radius: 999px; font-size: 13px; white-space: nowrap;
      color: var(--color-muted); text-decoration: none;
      &:hover { color: var(--color-ink); background: var(--color-panel-2); }
    }
    > a[aria-current], .more.here { color: var(--color-ink); background: var(--color-panel-2); }
  }

  /* Wide: the links. Folded (a phone, a narrow window, or a full header):
     the one button. After the rules that lay the links out, so it wins. */
  @mixin folded {
    .inline, .bar { display: none; }
    .folded { display: block; }
  }
  .folded { display: none; }
  @include narrow { @include folded; }
  .compact { @include folded; }

  .pop { position: relative; }
  button.more, button.burger {
    display: inline-flex; align-items: center; gap: 6px;
    min-height: 0; background: none; border: 0; font: inherit; font-weight: 400;
    cursor: pointer;
    &:hover { filter: none; }
  }
  .caret {
    width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent;
    border-top: 5px solid currentColor; transition: transform .15s;
  }
  .more[aria-expanded='true'] .caret { transform: rotate(180deg); }

  .burger {
    padding: 6px 10px; border-radius: 8px; font-size: 13px; color: var(--color-ink);
    box-shadow: inset 0 0 0 1px var(--color-line);
    &[aria-expanded='true'], &:hover { background: var(--color-panel-2); }
    @include phone { min-height: 38px; padding: 6px 12px; }
    .lines { display: grid; gap: 3px; i { display: block; width: 14px; height: 1.5px; background: currentColor; border-radius: 1px; } }
    /* The word goes on the smallest phones, and beside the developer's steps,
       where the three lines say it in less room. */
    @include tiny { .word { display: none; } }
  }
  .compact .burger {
    .word { display: none; }
  }

  .menu {
    position: absolute; right: 0; top: calc(100% + 8px); z-index: 50;
    display: grid; min-width: 190px; padding: 6px;
    background: var(--color-panel); border: 1px solid var(--color-line); border-radius: 10px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 45%);
    a {
      padding: 8px 10px; border-radius: 7px; font-size: 13.5px;
      color: var(--color-ink); text-decoration: none; white-space: nowrap;
      &:hover { background: var(--color-panel-2); }
      &[aria-current] { color: var(--color-accent); }
      @include phone { padding: 11px 12px; font-size: 15px; }
    }
    &.wide { min-width: 220px; }
    .language {
      padding: 2px 6px 4px;
      :global(.picker) { display: block; }
      :global(select) { width: 100%; font-size: 13.5px; padding: 7px 8px; }
    }
    .group {
      padding: 8px 10px 4px; font-size: 11px; font-weight: 650; letter-spacing: .06em;
      text-transform: uppercase; color: var(--color-muted);
      &:not(:first-child) { margin-top: 4px; border-top: 1px solid var(--color-line); padding-top: 10px; }
    }
  }
</style>
