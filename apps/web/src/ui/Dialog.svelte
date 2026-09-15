<script lang="ts">
  // A modal dialog: focus moves in and is trapped, Escape cancels, Enter
  // submits (outside text areas and buttons), the title bar drags it, and
  // focus returns to where it was on close. `scrim="clear"` keeps the canvas
  // visible for dialogs that preview on it.
  import { onMount, type Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";
  import { t } from "../lib/i18n";
  import { portal } from "./portal";

  let {
    title,
    width = 420,
    scrim = "dim",
    align = "center",
    testid,
    onclose,
    onsubmit,
    children,
    footer,
  }: {
    title: string;
    width?: number;
    scrim?: "dim" | "clear";
    align?: "center" | "right";
    testid?: string;
    onclose: () => void;
    onsubmit?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let box: HTMLDivElement;
  let offset = $state({ x: 0, y: 0 });
  const titleId = `ops-dlg-${Math.random().toString(36).slice(2, 8)}`;

  function focusables() {
    return [...box.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')].filter(
      (n) => !n.hasAttribute("disabled") && n.offsetParent !== null,
    );
  }

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    const first = box.querySelector<HTMLElement>("[data-autofocus]") ?? focusables().find((n) => n.tagName === "INPUT") ?? box;
    first.focus();
    if (first instanceof HTMLInputElement) first.select();
    return () => previous?.focus?.();
  });

  function onKey(e: KeyboardEvent) {
    // Nothing typed in a dialog reaches the app's shortcuts or tools.
    e.stopPropagation();
    if ((e.target as HTMLElement).closest(".ops-menu")) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "Enter" && onsubmit) {
      const el = e.target as HTMLElement;
      if (el.tagName === "TEXTAREA" || el.tagName === "BUTTON" || el.getAttribute("role") === "menuitem") return;
      e.preventDefault();
      // Let a field's own Enter handler commit its value first.
      queueMicrotask(() => onsubmit?.());
    } else if (e.key === "Tab") {
      const list = focusables();
      if (!list.length) return;
      const i = list.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && (i <= 0)) {
        e.preventDefault();
        list[list.length - 1].focus();
      } else if (!e.shiftKey && i === list.length - 1) {
        e.preventDefault();
        list[0].focus();
      }
    }
  }

  let drag: { x: number; y: number; ox: number; oy: number } | null = null;
  function dragDown(e: PointerEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y };
  }
  function dragMove(e: PointerEvent) {
    if (!drag) return;
    offset = { x: drag.ox + e.clientX - drag.x, y: drag.oy + e.clientY - drag.y };
  }
</script>

<div class="ops-dialog-scrim" class:clear={scrim === "clear"} class:right={align === "right"} use:portal>
  <div
    class="ops-dialog"
    bind:this={box}
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    data-testid={testid}
    style:width="{width}px"
    style:translate="{offset.x}px {offset.y}px"
    onkeydown={onKey}
  >
    <header class="ops-dialog__header" role="presentation" onpointerdown={dragDown} onpointermove={dragMove} onpointerup={() => (drag = null)}>
      <h2 id={titleId}>{title}</h2>
      <button type="button" class="close" aria-label={t("Close")} onclick={onclose}><X size={14} /></button>
    </header>
    <div class="ops-dialog__body">
      {@render children()}
    </div>
    {#if footer}
      <footer class="ops-dialog__footer">
        {@render footer()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .ops-dialog-scrim {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: grid;
    place-items: center;
    padding: var(--space-6);
    background: var(--scrim);
  }
  .ops-dialog-scrim.clear {
    background: transparent;
  }
  .ops-dialog-scrim.right {
    place-items: start end;
    padding: 96px 340px 24px 24px;
  }
  @media (max-width: 1100px) {
    .ops-dialog-scrim.right {
      padding-right: 24px;
    }
  }
  .ops-dialog {
    max-width: calc(100vw - 32px);
    max-height: calc(100dvh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface-raised);
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-xl);
    color: var(--text-body);
    outline: none;
    overflow: hidden;
  }
  .ops-dialog__header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 36px;
    padding: 0 6px 0 var(--space-4);
    border-bottom: var(--border-width) solid var(--border-hairline);
    cursor: grab;
    user-select: none;
    flex: 0 0 auto;
  }
  .ops-dialog__header:active {
    cursor: grabbing;
  }
  h2 {
    flex: 1;
    margin: 0;
    font: var(--weight-medium) var(--text-sm) / 1 var(--font-sans);
    color: var(--text-strong);
  }
  .close {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .close:hover {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .ops-dialog__body {
    padding: var(--space-4);
    overflow-y: auto;
    min-height: 0;
    font: var(--type-caption);
    font-size: var(--text-xs);
  }
  .ops-dialog__footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: var(--border-width) solid var(--border-hairline);
    flex: 0 0 auto;
  }
</style>
