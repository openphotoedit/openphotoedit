<script lang="ts" generics="T extends string | number">
  // A labelled select.
  let {
    label,
    value = $bindable(),
    options,
    testid,
  }: { label: string; value: T; options: { value: T; label: string }[]; testid?: string } = $props();
</script>

<label class="ops-topt__field">
  <span class="ops-topt__label">{label}</span>
  <select
    class="oa-input"
    data-testid={testid}
    value={String(value)}
    onchange={(e) => {
      const hit = options.find((o) => String(o.value) === e.currentTarget.value);
      if (hit) value = hit.value;
    }}
  >
    {#each options as o (o.value)}
      <option value={String(o.value)}>{o.label}</option>
    {/each}
  </select>
</label>
