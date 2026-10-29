<script>
  /** The frame around a page of reading: the same header as the tools, and a
   *  column of text narrow enough to follow. */
  import SiteHeader from '$lib/components/SiteHeader.svelte';
  import { localized, t } from '$lib/i18n.svelte.js';

  let { children } = $props();
</script>

<div class="shell">
  <SiteHeader tagline={t('app.tagline')} />

  <main>
    <article>
      {@render children()}
      <p class="open">
        <a class="button" href={localized('/')}>{t('links.open')}</a>
      </p>
    </article>
  </main>
</div>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  article {
    max-width: 68ch; margin: 8px auto 48px;
    font-size: 15px; line-height: 1.65;
    @include phone { font-size: 14.5px; margin-top: 0; }

    :global {
      h1 {
        margin: 0 0 14px; font-size: 26px; line-height: 1.25;
        font-weight: 650; letter-spacing: -0.015em;
        @include phone { font-size: 22px; }
      }
      h2 { margin: 34px 0 8px; font-size: 17px; font-weight: 650; }
      p, ul, ol { margin: 0 0 12px; }
      ul, ol { padding-left: 22px; }
      li { margin-bottom: 6px; }
      li strong { font-weight: 600; }
      a { color: var(--color-accent); }
      .lede { font-size: 16.5px; color: var(--color-ink); @include phone { font-size: 15.5px; } }
      .aside { color: var(--color-muted); font-size: 13.5px; }
      table { width: 100%; border-collapse: collapse; margin: 6px 0 16px; font-size: 14px; }
      th, td {
        text-align: left; vertical-align: top; padding: 7px 10px 7px 0;
        border-bottom: 1px solid var(--color-line);
      }
      th { font-weight: 600; color: var(--color-muted); font-size: 12.5px; }
      td:nth-child(2) { font-variant-numeric: tabular-nums; }
      code { font-size: 0.92em; color: var(--color-ink); }
      figure { margin: 18px 0 26px; }
      figcaption { margin-top: 8px; font-size: 13.5px; color: var(--color-muted); }
    }
  }

  .open {
    margin-top: 32px;
    .button {
      display: inline-block; padding: 9px 18px; border-radius: 8px;
      background: var(--color-accent); color: var(--color-accent-ink);
      font-weight: 600; text-decoration: none;
      &:hover { filter: brightness(1.08); }
    }
  }
</style>
