<script>
  /**
   * One tool of the grade, folded away until it is wanted: a row with its
   * name, a dot when it holds anything (so a look's work can be found), and
   * its controls under it. Sections sharing a `group` open one at a time.
   */
  import { t } from '$lib/i18n.svelte.js';

  let { title, edited = false, group = 'grade', children } = $props();
</script>

<details class="section" name={group}>
  <summary>
    <span class="title">{title}</span>
    {#if edited}<span class="dot" title={t('section.edited')} aria-label={t('section.edited')}></span>{/if}
    <span class="chevron" aria-hidden="true"></span>
  </summary>
  <div class="body">
    {@render children()}
  </div>
</details>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .section { border-top: 1px solid var(--color-line); }
  .section:last-of-type { border-bottom: 1px solid var(--color-line); }
  summary {
    display: flex; align-items: center; gap: 8px; padding: 10px 2px;
    cursor: pointer; list-style: none; user-select: none;
    @include phone { padding: 14px 2px; }
    &::-webkit-details-marker { display: none; }
    &:hover .title { color: var(--color-ink); }
    &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -2px; border-radius: 4px; }
  }
  .title { flex: 1; font-size: 12.5px; font-weight: 600; color: var(--color-muted);
           @include phone { font-size: 14px; } }
  .section[open] .title { color: var(--color-ink); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-accent); }
  .chevron {
    width: 7px; height: 7px; margin: 0 4px 3px 0;
    border-right: 1.5px solid var(--color-muted); border-bottom: 1.5px solid var(--color-muted);
    transform: rotate(45deg); transition: transform .15s;
  }
  .section[open] .chevron { transform: rotate(225deg); margin-bottom: -3px; }
  .body { padding: 0 0 14px; }
</style>
