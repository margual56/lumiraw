<script>
  /** Where the photograph goes, said once, next to where it is handed over. */
  import { t } from '$lib/i18n.svelte.js';
  import { install, installApp } from '$lib/install.svelte.js';
</script>

<p class="privacy">
  <svg aria-hidden="true" viewBox="0 0 24 24" width="15" height="15" fill="none"
       stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <rect x="4" y="10.5" width="16" height="10" rx="2.2" />
    <path d="M8 10.5V7a4 4 0 0 1 8 0v3.5" />
  </svg>
  <span>
    <strong>{t('privacy.title')}</strong> {t('privacy.body')}
    {#if install.available}
      <button type="button" class="install" onclick={installApp}>{t('install.button')}</button>
    {/if}
  </span>
</p>

<style lang="scss">
  @use '../../styles/type' as *;

  .privacy {
    display: flex; gap: 8px; align-items: flex-start;
    margin: 10px 2px 0; @include small(13px); color: var(--color-muted);
    svg { flex: none; margin-top: 2px; color: var(--color-ok); }
    strong { color: var(--color-ink); font-weight: 600; }
    /* A link-sized action inside the sentence, not a second call to action
       competing with the drop zone. */
    .install {
      display: inline; min-width: 0; min-height: 0; padding: 0; margin-left: 2px;
      background: none; border: 0; font: inherit; font-weight: 600;
      color: var(--color-accent); text-decoration: underline; text-underline-offset: 2px;
      cursor: pointer;
      &:hover { filter: brightness(1.15); }
    }
  }
</style>
