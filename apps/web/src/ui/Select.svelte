<script lang="ts" generics="V extends string | number">
  // A compact dropdown built on Menu, so groups (separators) and long lists
  // look the same on every platform. Arrow keys on the closed control step
  // through options, as a native select does.
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Menu from "./Menu.svelte";
  import type { MenuEntry } from "./menu";

  let {
    options,
    value,
    ariaLabel,
    width,
    disabled = false,
    testid,
    onchange,
  }: {
    options: ({ value: V; label: string; disabled?: boolean } | null)[];
    value: V;
    ariaLabel: string;
    width?: number;
    disabled?: boolean;
    testid?: string;
    onchange: (v: V) => void;
  } = $props();

  let btn = $state<HTMLButtonElement>() as HTMLButtonElement;
  let open = $state<DOMRect | null>(null);

  const current = $derived(options.find((o) => o && o.value === value));

  const items = $derived<MenuEntry[]>(
    options.map((o) =>
      o === null
        ? { type: "separator" as const }
        : { label: o.label, checked: o.value === value, radio: true, disabled: o.disabled, testid: testid ? `${testid}-${o.value}` : undefined, run: () => onchange(o.value) },
    ),
  );

  function key(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const real = options.filter((o): o is { value: V; label: string; disabled?: boolean } => !!o && !o.disabled);
      const i = real.findIndex((o) => o.value === value);
      const j = Math.max(0, Math.min(real.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)));
      if (real[j] && real[j].value !== value) onchange(real[j].value);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      open = btn.getBoundingClientRect();
    }
  }
</script>

<button
  type="button"
  class="ops-select"
  bind:this={btn}
  style:width={width ? `${width}px` : undefined}
  aria-label={ariaLabel}
  aria-haspopup="listbox"
  aria-expanded={!!open}
  data-testid={testid}
  data-value={value}
  {disabled}
  onpointerdown={(e) => {
    if (e.button !== 0 || disabled) return;
    e.preventDefault();
    btn.focus();
    open = open ? null : btn.getBoundingClientRect();
  }}
  onkeydown={key}
>
  <span class="text">{current?.label ?? ""}</span>
  <ChevronDown size={12} />
</button>

{#if open}
  <Menu
    {items}
    anchor={open}
    minWidth={Math.max(120, open.width)}
    ignore={btn}
    label={ariaLabel}
    autofocus={false}
    onclose={(reason) => {
      open = null;
      if (reason !== "outside") btn?.focus();
    }}
  />
{/if}

<style>
  .ops-select {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    height: var(--ops-control-h, 24px);
    min-width: 0;
    padding: 0 6px 0 8px;
    background: var(--bg-sunken);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-strong);
    cursor: default;
    transition: var(--transition-control);
  }
  .ops-select:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .ops-select[aria-expanded="true"] {
    border-color: var(--border-strong);
  }
  .ops-select:disabled {
    color: var(--text-faint);
  }
  .ops-select :global(svg) {
    flex: 0 0 auto;
    color: var(--text-muted);
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
