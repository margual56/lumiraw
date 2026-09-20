<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { toggleDetail, toggleSummary } from '$lib/format.js';

  const groups = $derived([...new Set(app.toggles.map((t) => t.group))]);

  /** The measured numbers behind the plain line, on hover, when they say
   *  more than it does. */
  function exact(toggle) {
    if (!toggle.available) return undefined;
    const detail = toggleDetail(toggle.detail);
    return detail && detail !== toggleSummary(toggle.detail) ? detail : undefined;
  }

  function flip(toggle) {
    app.settings.enabled[toggle.id] = !toggle.enabled;
  }
</script>

<div class="toggles">
  {#each groups as group}
    <div class="tgroup">
      <h3>{t(`group.${group.toLowerCase()}`)}</h3>
      {#each app.toggles.filter((t) => t.group === group) as toggle (toggle.id)}
        <button
          class="chip"
          type="button"
          aria-pressed={toggle.enabled}
          title={exact(toggle)}
          disabled={!toggle.available}
          onclick={() => flip(toggle)}
        >
          <span class="dot"></span>
          <span class="txt">
            <b>{t(`toggle.${toggle.id}`)}</b>
            <span>{toggle.available ? toggleSummary(toggle.detail) : t('reason.unavailable')}</span>
          </span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;
  .toggles { margin-top: 12px; }
  .tgroup {
    margin-bottom: 14px;
    > h3 {
      margin: 0 0 6px; @include section-label;
    }
  }
  .chip {
    display: flex; width: 100%; gap: 10px; align-items: flex-start;
    background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 8px;
    padding: 8px 10px; margin-bottom: 6px; cursor: pointer;
    @include phone { padding: 10px 12px; }
    color: var(--color-ink); font: inherit; font-weight: 400; text-align: left;
    &:hover { border-color: var(--color-muted); filter: none; }
    &:disabled { opacity: .45; cursor: default; }
    /* On, and off: the dot lights up and the name is struck through, so the
       state reads without depending on colour alone. */
    &[aria-pressed="true"] {
      border-color: #33413a;
      .dot { background: var(--color-ok); box-shadow: 0 0 7px rgb(111 202 139 / 50%); }
    }
    &[aria-pressed="false"] .txt b { color: var(--color-muted); text-decoration: line-through; }
  }
  .dot {
    width: 9px; height: 9px; border-radius: 50%; margin-top: 5px; flex: 0 0 auto;
    background: var(--color-line); box-shadow: inset 0 0 0 1px var(--color-line);
  }
  .txt {
    b { display: block; font-size: 12.5px; font-weight: 600; @include phone { font-size: 14px; } }
    span { color: var(--color-muted); @include small(11.5px); font-variant-numeric: tabular-nums; }
  }
</style>
