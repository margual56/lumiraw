<script>
  /** A section heading that can be linked to. */
  let { id, level = 2, children } = $props();
</script>

<svelte:element this={`h${level}`} {id} class="linkable">
  <a href="#{id}">{@render children()}<span class="mark" aria-hidden="true">#</span></a>
</svelte:element>

<style lang="scss">
  .linkable { scroll-margin-top: 16px; }
  /* Written against the heading, so it outranks the reading column's own
     rule for links (DocPage), which would otherwise colour every title. */
  .linkable a {
    color: inherit; text-decoration: none;
    &:hover .mark, &:focus-visible .mark { opacity: 1; }
  }
  .mark {
    margin-left: .35em; color: var(--color-muted); font-weight: 400;
    opacity: 0; transition: opacity .15s;
    @media (hover: none) { opacity: .5; }
  }
  .linkable:target .mark { opacity: 1; color: var(--color-accent); }
</style>
