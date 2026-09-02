<script>
  /** The roll, along the bottom: every raw dropped together. */
  import { app } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { openAt } from '$lib/roll.svelte.js';

  const kept = $derived(app.roll.filter((item) => item.keep).length);

  function setAll(keep) {
    for (const item of app.roll) item.keep = keep;
  }
</script>

{#if app.roll.length > 1}
  <section class="strip" aria-label={t('roll.title')}>
    <div class="head">
      <span>{t('roll.count', { kept, total: app.roll.length })}</span>
      <button type="button" class="link" onclick={() => setAll(kept < app.roll.length)}>
        {kept < app.roll.length ? t('roll.all') : t('roll.none_kept')}
      </button>
    </div>
    <ol>
      {#each app.roll as item, i (item.key)}
        <li class:current={i === app.rollAt} class:skipped={!item.keep} class:failed={item.failed}>
          <button type="button" class="frame" title={item.name} onclick={() => openAt(i)}>
            {#if item.thumb}
              <img src={item.thumb} alt={item.name} />
            {:else}
              <span class="name">{item.name}</span>
            {/if}
            {#if item.settings}<span class="edited" title={t('roll.edited')}>●</span>{/if}
          </button>
          <label class="keep" title={t('roll.keep')}>
            <input type="checkbox" bind:checked={item.keep} />
          </label>
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style lang="scss">
  @use '../../styles/type' as *;
  @use '../../styles/breakpoints' as *;

  .strip {
    border-top: 1px solid var(--color-line); background: var(--color-panel);
    padding: 6px 22px 8px;
    @include phone { padding: 6px 12px 8px; }
  }
  .head {
    display: flex; gap: 12px; align-items: baseline;
    @include small(12px); color: var(--color-muted); margin-bottom: 6px;
    .link {
      all: unset; cursor: pointer; color: var(--color-accent); @include small(12px);
      @include phone { padding: 8px 0; }
      &:hover { text-decoration: underline; }
    }
  }
  ol {
    display: flex; gap: 8px; margin: 0; padding: 0 0 4px; list-style: none;
    overflow-x: auto;
  }
  li { position: relative; flex: none; }
  .frame {
    all: unset; box-sizing: border-box; cursor: pointer; display: grid; place-items: center;
    width: 96px; height: 64px; border-radius: 6px; overflow: hidden;
    background: #000; border: 2px solid transparent;
    img { max-width: 100%; max-height: 100%; display: block; }
    .name { font-size: 10px; color: var(--color-muted); padding: 4px; word-break: break-all; }
    &:hover { border-color: var(--color-muted); }
  }
  .current .frame { border-color: var(--color-accent); }
  .skipped .frame { opacity: .4; }
  .failed .frame { border-color: #b04a4a; }
  .edited {
    position: absolute; left: 6px; top: 3px; font-size: 10px; color: var(--color-accent);
    text-shadow: 0 0 3px #000;
  }
  .keep {
    position: absolute; right: 4px; top: 3px; line-height: 0;
    input {
      margin: 0; accent-color: var(--color-accent); cursor: pointer;
      @include phone { width: 20px; height: 20px; }
    }
  }
</style>
