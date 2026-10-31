<script>
  /** A link from the step on screen to its section of the tutorial on the
   *  blog, which explains it and has it to try (blog.lumiraw.app/tutorial,
   *  anchored by the names in STEP_NAMES). Not shown inside the tutorial
   *  itself, where the explanation is the page around it. */
  import { onMount } from 'svelte';
  import { app } from '$lib/state.svelte.js';
  import { i18n, t } from '$lib/i18n.svelte.js';
  import { embedded, STEP_NAMES } from '$lib/demo.js';
  import { ELSEWHERE } from '$lib/pages.js';

  let shown = $state(false);
  onMount(() => { shown = !embedded(); });

  const blog = ELSEWHERE.find((link) => link.key === 'links.blog').href;
  const name = $derived(Object.keys(STEP_NAMES).find((key) => STEP_NAMES[key] === app.step));
</script>

{#if shown && name}
  <a class="guide" href="{blog[i18n.locale] ?? blog.en}/tutorial#{name}" target="_blank" rel="noopener"
     title={t('step.guide.why')}>{t('step.guide')} ↗</a>
{/if}

<style lang="scss">
  .guide {
    float: right; margin-left: 12px; font-size: 12.5px; font-weight: 500;
    color: var(--color-muted); text-decoration: none;
    &:hover, &:focus-visible { color: var(--color-accent); text-decoration: underline; }
  }
</style>
