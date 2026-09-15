<script lang="ts">
  let { label, checked = $bindable(), testid }: { label: string; checked: boolean; testid?: string } = $props();
</script>

<label class="ops-topt__field">
  <input class="oa-checkbox" type="checkbox" bind:checked data-testid={testid} />
  <span>{label}</span>
</label>
