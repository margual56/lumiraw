<script>
  // The tokens and Tailwind first, then everything written in terms of them.
  import '../theme.css';
  import '../app.scss';
  import { detectLocale, setLocale } from '$lib/i18n.svelte.js';

  let { children } = $props();

  // Locale detection touches localStorage and navigator, so it waits for the
  // browser rather than running while the shell is prerendered.
  $effect(() => setLocale(detectLocale()));
</script>

{@render children()}
