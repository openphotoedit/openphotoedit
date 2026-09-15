<script lang="ts" generics="T extends string">
  // Icon (or text) buttons where exactly one is pressed.
  import type { Component } from "svelte";
  type Icon = Component<{ size?: number | string }>;
  let {
    label,
    value = $bindable(),
    options,
    testid,
  }: { label?: string; value: T; options: { value: T; label: string; icon?: Icon }[]; testid?: string } = $props();
</script>

<div class="ops-topt__field" role="group" aria-label={label}>
  {#if label}<span class="ops-topt__label">{label}</span>{/if}
  <div class="ops-topt__seg" data-testid={testid}>
    {#each options as o (o.value)}
      <button type="button" aria-pressed={value === o.value} title={o.label} aria-label={o.label} onclick={() => (value = o.value)}>
        {#if o.icon}<o.icon size={16} />{:else}{o.label}{/if}
      </button>
    {/each}
  </div>
</div>
