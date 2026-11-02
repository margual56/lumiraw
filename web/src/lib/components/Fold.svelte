<script>
  import { readStored } from '$lib/stored.js';

  /** A section closed until asked for, its main fact in the heading. Which
   *  ones are open is remembered in this browser. */
  let { id, title, peek = '', children } = $props();

  const STORE = 'lumiraw.folds';
  const opened = () => readStored(STORE);
  const keep = (event) => {
    try {
      localStorage.setItem(STORE, JSON.stringify({ ...opened(), [id]: event.currentTarget.open }));
    } catch {
      // Private window or storage refused: it just is not remembered.
    }
  };
</script>

<details class="fold" open={opened()[id] ?? false} ontoggle={keep}>
  <summary><strong>{title}</strong>{#if peek}<span>{peek}</span>{/if}</summary>
  <div class="inside">{@render children()}</div>
</details>

<style lang="scss">
  .fold { border-top: 1px solid var(--color-line); }
  summary {
    display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 10px; padding: 11px 0;
    cursor: pointer; list-style: none; font-size: 13.5px;
    &::-webkit-details-marker { display: none; }
    &::before {
      content: ''; align-self: center; width: 6px; height: 6px; margin: 0 3px 0 2px;
      border-right: 1.5px solid var(--color-muted); border-bottom: 1.5px solid var(--color-muted);
      transform: rotate(-45deg); transition: transform .12s;
    }
    &:hover strong { color: var(--color-accent); }
    &:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
  }
  [open] > summary::before { transform: rotate(45deg); }
  strong { font-weight: 600; }
  span { color: var(--color-muted); font-size: 13px; }
  .inside { padding-bottom: 14px; }
</style>
