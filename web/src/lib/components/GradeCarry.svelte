<script>
  /** What the next photograph will inherit, said before it is dropped. */
  import { app, hasGrade } from '$lib/state.svelte.js';
  import { t } from '$lib/i18n.svelte.js';
  import { lookLabel } from '$lib/format.js';

  const s = $derived(app.settings);
  const percent = (v) => `${Math.round(v * 100)}%`;

  const parts = $derived.by(() => {
    const out = [];
    if (s.preset && s.preset !== 'natural') out.push(t(`render.${s.preset}`));
    if (s.look) {
      const look = app.looks.find((l) => l.id === s.look);
      out.push(lookLabel(s.look, look?.label ?? s.look));
    }
    const c = s.curves ?? {};
    if (['rgb', 'r', 'g', 'b'].some((k) => (c[k] ?? []).length)) out.push(t('carry.curve'));
    if (Object.keys(s.mixer ?? {}).length) out.push(t('mixer.title').toLowerCase());
    if (app.lut && s.lut > 0) out.push(app.lut.title || app.lut.name);
    if (s.monochrome > 0) out.push(`${t('grade.monochrome').toLowerCase()} ${percent(s.monochrome)}`);
    if (s.vignette > 0) out.push(`${t('grade.vignette').toLowerCase()} ${percent(s.vignette)}`);
    if (s.grain > 0) out.push(`${t('grade.grain').toLowerCase()} ${percent(s.grain)}`);
    return out;
  });
</script>

{#if app.id && hasGrade(app.settings)}
  <label class="carry">
    <input type="checkbox" bind:checked={app.keepGrade} />
    <span>
      <strong>{t('carry.title')}</strong>
      <span class="what">{parts.join(' · ')}</span>
    </span>
  </label>
{/if}

<style lang="scss">
  @use '../../styles/surfaces' as *;

  .carry {
    @include surface;
    display: flex; gap: 10px; align-items: flex-start;
    margin-bottom: 12px; padding: 12px 14px; cursor: pointer;
    input { margin-top: 3px; accent-color: var(--color-accent); }
    strong { display: block; font-size: 13.5px; font-weight: 600; }
    .what { color: var(--color-muted); font-size: 12.5px; }
  }
</style>
