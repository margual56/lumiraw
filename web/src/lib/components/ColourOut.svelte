<script>
  /**
   * Colour out: the space the download is written in, the printer's profile
   * CMYK goes through, and a proof of the picture through that profile.
   * The profile, like a .cube, is read in this tab and goes nowhere else.
   */
  import { app, canCmyk, exportSpace } from '$lib/state.svelte.js';
  import { t, n } from '$lib/i18n.svelte.js';
  import { errorText } from '$lib/format.js';
  import { profileLoad, profileClear } from '$lib/api.js';

  let input = $state(null);
  let problem = $state('');
  let reading = $state(false);

  const profile = $derived(app.colour.profile);
  const cmyk = $derived(exportSpace() !== 'srgb');

  async function load(event) {
    const file = event.currentTarget.files?.[0];
    if (!file) return;
    problem = '';
    reading = true;
    try {
      const info = await profileLoad(file);
      app.colour.profile = { ...info, name: info.name || file.name };
    } catch (err) {
      // Whatever was loaded is still loaded, in the module and here.
      problem = errorText(err);
    } finally {
      reading = false;
      if (input) input.value = '';
    }
  }

  async function forget() {
    problem = '';
    app.colour.profile = await profileClear();
  }
</script>

<div class="field">
  <label for="space">{t('colour.space')}</label>
  <select id="space" value={cmyk ? 'cmyk' : 'srgb'}
          onchange={(e) => (app.colour.space = e.currentTarget.value)}>
    <option value="srgb">{t('colour.srgb')}</option>
    <option value="cmyk" disabled={!canCmyk()}>{t('colour.cmyk')}</option>
  </select>
</div>

<div class="print">
  <p class="label">{t('colour.profile')}</p>
  {#if profile}
    <p class="loaded">
      {#if profile.builtin}
        {t('colour.builtin', { name: profile.name })}
      {:else}
        {t('colour.loaded', { name: profile.name, kind: profile.kind.toUpperCase(),
                              kb: n(profile.bytes / 1000, 0) })}
      {/if}
    </p>
  {/if}
  {#if cmyk}
    <p class="hint small">{t('colour.cmykHint')}</p>
  {:else if !canCmyk()}
    <p class="hint small">{t('colour.rgbPrinter')}</p>
  {/if}

  <div class="buttons">
    <button class="ghost" type="button" disabled={reading} onclick={() => input?.click()}>
      {profile && !profile.builtin ? t('colour.another') : t('colour.load')}
    </button>
    {#if profile && !profile.builtin}
      <button class="ghost" type="button" onclick={forget}>{t('colour.forget')}</button>
    {/if}
  </div>

  {#if cmyk || app.colour.proof}
    <div class="field">
      <label for="intent">{t('colour.intent')}</label>
      <select id="intent" bind:value={app.colour.intent}>
        <option value="perceptual">{t('colour.perceptual')}</option>
        <option value="relative">{t('colour.relative')}</option>
      </select>
    </div>
  {/if}

  <label class="check">
    <input type="checkbox" bind:checked={app.colour.proof} />
    <span>{t('colour.proof')}<em>{t('colour.proofHint')}</em></span>
  </label>

  {#if problem}<p class="problem">{problem}</p>{/if}
</div>

<input bind:this={input} type="file" accept=".icc,.icm,application/vnd.iccprofile"
       onchange={load} hidden />

<style lang="scss">
  @use '../../styles/type' as *;
  .print { margin: 12px 0 14px; }
  .label { margin: 0 0 6px; @include small(11.8px); color: var(--color-muted); }
  .small { display: block; margin: 4px 0 8px; @include small(11.5px); }
  .loaded {
    margin: 0 0 8px; padding: 8px 10px; border-radius: 8px;
    background: var(--color-panel-2); color: var(--color-muted);
    font-size: 11.8px; line-height: 1.5; overflow-wrap: anywhere;
  }
  .buttons { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .problem { margin: 8px 0 0; color: var(--color-warn, #e5a03c); @include small(11.8px); }
</style>
