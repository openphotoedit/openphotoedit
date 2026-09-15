<script lang="ts">
  // A square icon control with an instant tooltip that names the action and
  // its shortcut. `pressed` makes it a toggle (aria-pressed).
  import type { Snippet } from "svelte";
  import { tooltip } from "./tooltip";

  let {
    label,
    shortcut,
    size = "md",
    selected = false,
    pressed,
    disabled = false,
    placement = "bottom",
    testid,
    onclick,
    oncontextmenu,
    onpointerdown,
    children,
    class: klass = "",
  }: {
    label: string;
    shortcut?: string;
    size?: "xs" | "sm" | "md";
    selected?: boolean;
    pressed?: boolean;
    disabled?: boolean;
    placement?: "bottom" | "right" | "top" | "left";
    testid?: string;
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
    children: Snippet;
    class?: string;
  } = $props();
</script>

<button
  type="button"
  class="ops-icon-btn ops-icon-btn--{size} {klass}"
  class:selected
  aria-label={label}
  aria-pressed={pressed}
  data-testid={testid}
  {disabled}
  use:tooltip={{ text: label, shortcut, placement }}
  {onclick}
  {oncontextmenu}
  {onpointerdown}
>
  {@render children()}
</button>

<style>
  .ops-icon-btn {
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
    padding: 0;
    color: var(--text-muted);
    background: transparent;
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-xs);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .ops-icon-btn--xs {
    width: 20px;
    height: 20px;
  }
  .ops-icon-btn--sm {
    width: 24px;
    height: 24px;
  }
  .ops-icon-btn--md {
    width: 28px;
    height: 28px;
  }
  .ops-icon-btn:hover:not(:disabled) {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .ops-icon-btn[aria-pressed="true"],
  .ops-icon-btn.selected {
    background: var(--surface-active);
    color: var(--text-strong);
    border-color: var(--border-hairline);
  }
  .ops-icon-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
