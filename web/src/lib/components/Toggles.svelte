<script>
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { toggleDetail } from '$lib/format.js';

  const groups = $derived([...new Set(app.toggles.map((t) => t.group))]);

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
          disabled={!toggle.available}
          onclick={() => flip(toggle)}
        >
          <span class="dot"></span>
          <span class="txt">
            <b>{t(`toggle.${toggle.id}`)}</b>
            <span>{toggle.available ? toggleDetail(toggle.detail) : t('reason.unavailable')}</span>
          </span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style lang="scss">
  .toggles { margin-top: 12px; }
  .tgroup {
    margin-bottom: 14px;
    > h3 {
      margin: 0 0 6px; font-size: 10.5px; text-transform: uppercase;
      letter-spacing: .09em; color: var(--color-muted); font-weight: 600;
    }
  }
  .chip {
    display: flex; width: 100%; gap: 10px; align-items: flex-start;
    background: var(--color-panel-2); border: 1px solid var(--color-line); border-radius: 8px;
    padding: 8px 10px; margin-bottom: 6px; cursor: pointer;
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
    b { display: block; font-size: 12.5px; font-weight: 600; }
    span { color: var(--color-muted); font-size: 11.5px; font-variant-numeric: tabular-nums; }
  }
</style>
