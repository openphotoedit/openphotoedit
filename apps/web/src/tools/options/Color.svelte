<script lang="ts">
  // A colour well. `value` null means "follow the foreground colour"; the
  // well shows that colour until one is picked.
  import type { Rgba8 } from "../../engine/types";
  import { fromHex, toHex } from "../common";

  let {
    label,
    value = $bindable(),
    fallback,
    testid,
  }: { label: string; value: Rgba8 | null; fallback?: Rgba8; testid?: string } = $props();

  const shown = $derived(value ?? fallback ?? { r: 0, g: 0, b: 0, a: 255 });
</script>

<label class="ops-topt__field">
  <span class="ops-topt__label">{label}</span>
  <input
    class="ops-topt__color"
    type="color"
    aria-label={label}
    data-testid={testid}
    value={toHex(shown)}
    oninput={(e) => (value = fromHex(e.currentTarget.value, shown.a ?? 255))}
  />
</label>
