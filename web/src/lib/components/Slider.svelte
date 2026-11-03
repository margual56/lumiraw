<script>
  // Without `oninput`, dragging writes straight to `value`. With it, the
  // caller decides what a drag changes, and `onchange` hears the release.
  // `onactive` is told whether the thumb is held, for pictures that follow it.
  let {
    label,
    value = $bindable(0),
    min = -1, max = 1, step = 0.01,
    format = (v) => v.toFixed(2),
    tone = '',
    oninput = null,
    onchange = null,
    onactive = null,
  } = $props();

  const id = $props.id();

  let shown = $derived(value);

  function input(event) {
    shown = Number(event.currentTarget.value);
    if (oninput) oninput(shown);
    else value = shown;
  }
</script>

<div class="field">
  <div class="row"><label for={id}>{label}</label><output for={id}>{format(shown)}</output></div>
  <input
    {id} type="range" {min} {max} {step} class={tone}
    value={shown}
    oninput={input}
    onchange={(e) => onchange?.(Number(e.currentTarget.value))}
    onpointerdown={() => onactive?.(true)}
    onpointerup={() => onactive?.(false)}
    onpointercancel={() => onactive?.(false)}
    onlostpointercapture={() => onactive?.(false)}
  />
</div>
