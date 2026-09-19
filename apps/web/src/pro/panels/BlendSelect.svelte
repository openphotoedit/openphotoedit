<script lang="ts">
  // The Layers panel's blend-mode dropdown. It looks and behaves like
  // ui/Select, and also reports the option under the pointer (or keyboard
  // focus) while the menu is open, so the panel can preview that mode on the
  // canvas as Photoshop does. `onpreview(null)` means "put it back".
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Menu from "../../ui/Menu.svelte";
  import type { MenuEntry } from "../../ui/menu";

  let {
    options,
    value,
    ariaLabel,
    disabled = false,
    testid,
    onchange,
    onpreview,
  }: {
    options: ({ value: string; label: string } | null)[];
    value: string;
    ariaLabel: string;
    disabled?: boolean;
    testid: string;
    onchange: (v: string) => void;
    onpreview: (v: string | null) => void;
  } = $props();

  let btn = $state<HTMLButtonElement>() as HTMLButtonElement;
  let open = $state<DOMRect | null>(null);
  let previewing: string | null = null;

  const current = $derived(options.find((o) => o && o.value === value));
  const items = $derived<MenuEntry[]>(
    options.map((o) => (o === null ? { type: "separator" as const } : { label: o.label, checked: o.value === value, radio: true, testid: `${testid}-${o.value}`, run: () => onchange(o.value) })),
  );

  // Menu moves focus to the item under the pointer and to the one the arrow
  // keys reach, so following focus covers both.
  $effect(() => {
    if (!open) return;
    const prefix = `${testid}-`;
    const onFocus = (e: FocusEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-testid]");
      const id = el?.dataset.testid;
      if (!id?.startsWith(prefix)) return;
      const v = id.slice(prefix.length);
      if (v === previewing) return;
      previewing = v;
      onpreview(v === value ? null : v);
    };
    document.addEventListener("focusin", onFocus);
    return () => document.removeEventListener("focusin", onFocus);
  });

  function close(reason: string) {
    open = null;
    // Choosing an item commits through onchange; anything else puts the
    // layer back as it was.
    if (reason !== "activate" && previewing !== null) onpreview(null);
    previewing = null;
    if (reason !== "outside") btn?.focus();
  }

  function key(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const real = options.filter((o): o is { value: string; label: string } => !!o);
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
  <Menu {items} anchor={open} minWidth={Math.max(120, open.width)} ignore={btn} label={ariaLabel} autofocus={false} onclose={close} />
{/if}

<style>
  .ops-select {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    width: 100%;
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
  .ops-select:hover:not(:disabled),
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
