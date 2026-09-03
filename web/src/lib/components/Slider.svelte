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

  // Ties the label to the range, so it is read out with it and a click on
  // the label reaches it.
  const id = $props.id();

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
  <div class="row"><label for={id}>{label}</label><output for={id}>{format(shown)}</output></div>
  <input
    {id} type="range" {min} {max} {step} class={tone}
    value={shown}
    oninput={input}
    onchange={commit}
  />
</div>
