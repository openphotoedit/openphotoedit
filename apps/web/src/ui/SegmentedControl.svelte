<script lang="ts" generics="V extends string | number">
  // A row of mutually exclusive options (a radio group that looks like
  // joined buttons). Arrow keys move the selection.
  let {
    options,
    value,
    ariaLabel,
    testid,
    size = "sm",
    onchange,
  }: {
    options: { value: V; label: string; disabled?: boolean }[];
    value: V;
    ariaLabel: string;
    testid?: string;
    size?: "xs" | "sm";
    onchange: (v: V) => void;
  } = $props();

  let group: HTMLDivElement;

  function key(e: KeyboardEvent, i: number) {
    if (!["ArrowRight", "ArrowLeft", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    e.preventDefault();
    const dir = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1;
    let j = i;
    for (let k = 0; k < options.length; k++) {
      j = (j + dir + options.length) % options.length;
      if (!options[j].disabled) break;
    }
    onchange(options[j].value);
    group.querySelectorAll<HTMLButtonElement>("button")[j]?.focus();
  }
</script>

<div class="ops-seg ops-seg--{size}" role="radiogroup" aria-label={ariaLabel} data-testid={testid} bind:this={group}>
  {#each options as o, i (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={o.value === value}
      tabindex={o.value === value ? 0 : -1}
      disabled={o.disabled}
      data-testid={testid ? `${testid}-${o.value}` : undefined}
      onclick={() => onchange(o.value)}
      onkeydown={(e) => key(e, i)}>{o.label}</button
    >
  {/each}
</div>

<style>
  .ops-seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    background: var(--bg-sunken);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
  }
  button {
    flex: 1;
    height: 20px;
    padding: 0 8px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-sans);
    color: var(--text-muted);
    white-space: nowrap;
    cursor: default;
    transition: var(--transition-control);
  }
  .ops-seg--sm button {
    font-size: var(--text-xs);
  }
  button:hover:not(:disabled) {
    color: var(--text-strong);
  }
  button[aria-checked="true"] {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  button:disabled {
    opacity: 0.4;
  }
  button:focus-visible {
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }
</style>
