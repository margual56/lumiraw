<script>
  /** Where the photograph goes, stated plainly on the step that asks for it. */
  import { t } from '$lib/i18n.svelte.js';
</script>

<section class="privacy">
  <div class="mark" aria-hidden="true">
    <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor"
         stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="4" y="10.5" width="16" height="10" rx="2.2" />
      <path d="M8 10.5V7a4 4 0 0 1 8 0v3.5" />
    </svg>
  </div>
  <div class="body">
    <h2>{t('privacy.title')}</h2>
    <p>{t('privacy.body')}</p>
    <p class="fine">{t('privacy.downloads')}</p>
  </div>
</section>

<style>
  .privacy {
    display: flex; gap: 16px; align-items: flex-start;
    padding: 18px 20px; margin-bottom: 16px;
    border: 1.5px solid var(--ok); border-radius: var(--radius);
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--ok) 9%, transparent), transparent),
      var(--panel);
  }
  .mark {
    flex: none; display: grid; place-items: center;
    width: 42px; height: 42px; border-radius: 50%;
    color: var(--ok); background: color-mix(in srgb, var(--ok) 14%, transparent);
  }
  .body { min-width: 0; }
  h2 {
    margin: 0 0 4px; font-size: 17px; font-weight: 650; letter-spacing: -0.01em;
    color: var(--ok);
  }
  p { margin: 0; font-size: 14.5px; line-height: 1.55; }
  .fine { margin-top: 6px; font-size: 12.5px; color: var(--muted); }

  @media (max-width: 640px) {
    .privacy { padding: 14px; gap: 12px; }
    .mark { width: 34px; height: 34px; }
    h2 { font-size: 15.5px; }
    p { font-size: 13.5px; }
  }
</style>
