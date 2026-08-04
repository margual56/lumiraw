<script>
  /** Load a `.cube` file, and say plainly where it goes. */
  import Slider from '$lib/components/Slider.svelte';
  import { app } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { lutLoad, lutClear } from '$lib/api.js';

  let input = $state(null);
  let problem = $state('');

  async function load(event) {
    const file = event.currentTarget.files?.[0];
    if (!file) return;
    problem = '';
    try {
      const info = await lutLoad(file);
      app.lut = { ...info, name: file.name };
      // A table nobody can see is a table nobody loaded, so it arrives on.
      app.settings.lut_id = info.id;
      app.settings.lut = app.settings.lut > 0 ? app.settings.lut : 1;
    } catch (err) {
      // Whatever was loaded is still loaded, in the module and here.
      problem = errorText(err);
    } finally {
      if (input) input.value = '';
    }
  }

  async function forget() {
    await lutClear();
    app.lut = null;
    app.settings.lut = 0;
    app.settings.lut_id = '';
    problem = '';
  }
</script>

<div class="cube">
  <h3>{t('cube.title')}</h3>
  <p class="lede small">{@html t('cube.lede')}</p>

  {#if app.lut}
    <p class="loaded">
      {t('cube.loaded', { name: app.lut.title || app.lut.name, size: app.lut.size,
                          kb: n(app.lut.bytes / 1000, 0) })}
    </p>
    <Slider label={t('cube.strength')} bind:value={app.settings.lut}
            min={0} max={1} step={0.05}
            format={(v) => (v > 0 ? `${Math.round(v * 100)}%` : t('grade.off'))} />
    <button class="ghost" type="button" onclick={forget}>{t('cube.forget')}</button>
  {:else}
    <button class="ghost" type="button" onclick={() => input?.click()}>
      {t('cube.load')}
    </button>
  {/if}

  {#if problem}<p class="problem">{problem}</p>{/if}

  <input bind:this={input} type="file" accept=".cube" onchange={load} hidden />
</div>

<style lang="scss">
  .cube { margin-top: 18px; }
  h3 {
    margin: 0 0 4px; font-size: 10.5px; text-transform: uppercase;
    letter-spacing: .09em; color: var(--color-muted); font-weight: 600;
  }
  .small { font-size: 11.8px; margin-bottom: 10px; }
  .loaded {
    margin: 0 0 10px; padding: 8px 10px; border-radius: 8px;
    background: var(--color-panel-2); color: var(--color-muted);
    font-size: 11.8px; line-height: 1.5;
  }
  .problem { margin: 10px 0 0; color: var(--color-warn, #e5a03c); font-size: 11.8px; }
</style>
