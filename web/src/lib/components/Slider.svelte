<script>
  /** A labelled range. */
  let {
    label,
    value = $bindable(0),
    min = -1, max = 1, step = 0.01,
    format = (v) => v.toFixed(2),
    tone = '',
    commitOnRelease = false,
    onpreview = null,
  } = $props();

  let draft = $state(value);
  $effect(() => { draft = value; });     // follow changes made elsewhere

  const shown = $derived(commitOnRelease ? draft : value);

  function input(event) {
    const next = Number(event.currentTarget.value);
    if (!commitOnRelease) {
      value = next;
      return;
    }
    draft = next;
    onpreview?.(next);
  }

  function commit(event) {
    if (commitOnRelease) value = Number(event.currentTarget.value);
  }
</script>

<div class="field">
  <div class="row"><label>{label}</label><output>{format(shown)}</output></div>
  <input
    type="range" {min} {max} {step} class={tone}
    value={shown}
    oninput={input}
    onchange={commit}
  />
</div>
