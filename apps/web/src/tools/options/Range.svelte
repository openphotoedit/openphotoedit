<script lang="ts">
  // Label, slider and number field bound to one value. `percent` shows a
  // 0..1 value as 0..100.
  let {
    label,
    value = $bindable(),
    min = 0,
    max = 100,
    step = 1,
    percent = false,
    unit = "",
    testid,
  }: { label: string; value: number; min?: number; max?: number; step?: number; percent?: boolean; unit?: string; testid?: string } = $props();

  const k = $derived(percent ? 100 : 1);
  const shown = $derived(Math.round(value * k * 100) / 100);

  function set(v: number) {
    if (!Number.isFinite(v)) return;
    value = Math.min(max, Math.max(min, v / k));
  }
</script>

<label class="ops-topt__field">
  <span class="ops-topt__label">{label}</span>
  <input type="range" min={min * k} max={max * k} step={step} value={shown} oninput={(e) => set(Number(e.currentTarget.value))} data-testid={testid} />
  <input
    class="oa-input ops-topt__num"
    type="number"
    min={min * k}
    max={max * k}
    step={step}
    value={shown}
    aria-label={label}
    onchange={(e) => set(Number(e.currentTarget.value))}
  />{#if unit}<span class="ops-topt__label">{unit}</span>{/if}
</label>
