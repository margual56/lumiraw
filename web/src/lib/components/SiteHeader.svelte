<script>
  /** Every page's header: the name, which is the way home (and the page's
   *  own heading on the developer), what this page is, the version, anything
   *  the page puts beside them, and the way to the other pages. */
  import SiteNav from '$lib/components/SiteNav.svelte';
  import { localized } from '$lib/i18n.svelte.js';
  import { NAME } from '$lib/brand.js';
  import { app } from '$lib/state.svelte.js';

  /** `home`: the name is this page's heading rather than a link to it.
   *  `compact`: the navigation folds into its menu, for a full header.
   *  `bare`: no navigation at all, for the developer inside a blog post. */
  let { tagline, home = false, compact = false, bare = false, children } = $props();
</script>

<header>
  <svelte:element this={home ? 'h1' : 'a'} class="brand" href={home ? undefined : localized('/')}>
    {NAME}
    <span>
      <span class="tagline">{tagline}</span>
      {#if app.version}<span class="ver">v{app.version}</span>{/if}
    </span>
  </svelte:element>
  {@render children?.()}
  {#if !bare}<SiteNav {compact} />{/if}
</header>

<style lang="scss">
  @use '../../styles/breakpoints' as *;

  /* On the developer, the steps want the room on a phone: the tagline goes and
     the version stays, since the version is the half worth the screen space. */
  h1.brand .tagline { @include phone { display: none; } }

  a.brand {
    color: inherit; text-decoration: none;
    &:hover {
      color: var(--color-accent);
      .tagline, .ver { color: var(--color-muted); }
    }
  }
</style>
