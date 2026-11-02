<script>
  import { goto } from '$app/navigation';
  import Calculators from '$lib/components/Calculators.svelte';
  import { localized } from '$lib/i18n.svelte.js';
  import { SKY } from '$lib/pages.js';

  // The sky's calculators were here once: their old links go on to its page.
  $effect(() => {
    if (SKY.includes(location.hash.slice(1))) goto(localized('/sky') + location.hash, { replaceState: true });
  });
</script>

<Calculators tools={['dof', 'nd']} page="calc" path="/calculators" />
