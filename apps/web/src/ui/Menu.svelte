<script lang="ts">
  // A popup menu with submenus, checks, shortcuts and disabled states.
  // Keyboard: arrows move, Right opens a submenu, Left closes one, Enter or
  // Space activates, Escape closes one level, typing a letter jumps.
  import { onMount, tick } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Menu from "./Menu.svelte";
  import { formatShortcut } from "./platform";
  import { portal } from "./portal";
  import { tooltip } from "./tooltip";
  import { isItem, subItems, type MenuCloseReason, type MenuEntry, type MenuItem } from "./menu";

  let {
    items,
    x = 0,
    y = 0,
    anchor = null,
    placement = "below",
    level = 0,
    autofocus = false,
    minWidth = 180,
    ignore = null,
    label,
    testid,
    onclose,
    onnavigate,
  }: {
    items: MenuEntry[];
    x?: number;
    y?: number;
    anchor?: DOMRect | null;
    placement?: "below" | "right";
    level?: number;
    autofocus?: boolean;
    minWidth?: number;
    ignore?: HTMLElement | null;
    label?: string;
    testid?: string;
    onclose: (reason: MenuCloseReason) => void;
    onnavigate?: (dir: -1 | 1) => void;
  } = $props();

  let el: HTMLDivElement;
  let active = $state(-1);
  let sub = $state<{ index: number; rect: DOMRect; focus: boolean } | null>(null);
  let pos = $state({ left: -9999, top: -9999 });
  let hoverTimer = 0;

  const enabled = (i: number) => {
    const e = items[i];
    return !!e && isItem(e) && !e.disabled;
  };

  function itemEls() {
    return [...el.querySelectorAll<HTMLElement>(":scope > .ops-menu__list > [data-index]")];
  }

  function focusItem(i: number) {
    active = i;
    const node = itemEls().find((n) => Number(n.dataset.index) === i);
    node?.focus({ preventScroll: false });
  }

  function step(dir: 1 | -1) {
    const n = items.length;
    let i = active;
    for (let k = 0; k < n; k++) {
      i = (i + dir + n) % n;
      if (enabled(i)) {
        focusItem(i);
        return;
      }
    }
  }

  function openSub(i: number, focus: boolean) {
    const node = itemEls().find((n) => Number(n.dataset.index) === i);
    if (!node) return;
    sub = { index: i, rect: node.getBoundingClientRect(), focus };
  }

  function activate(i: number) {
    const e = items[i];
    if (!e || !isItem(e) || e.disabled) return;
    if (e.submenu) {
      openSub(i, true);
      return;
    }
    onclose("activate");
    // Run after the menu has gone so dialogs can take focus.
    queueMicrotask(() => e.run?.());
  }

  function onKey(e: KeyboardEvent) {
    // Only the deepest open menu handles keys.
    if (sub) return;
    e.stopPropagation();
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        step(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        step(-1);
        break;
      case "Home":
        e.preventDefault();
        active = -1;
        step(1);
        break;
      case "End":
        e.preventDefault();
        active = items.length;
        step(-1);
        break;
      case "ArrowRight": {
        e.preventDefault();
        const it = items[active];
        if (it && isItem(it) && it.submenu && !it.disabled) openSub(active, true);
        else onnavigate?.(1);
        break;
      }
      case "ArrowLeft":
        e.preventDefault();
        if (level > 0) onclose("left");
        else onnavigate?.(-1);
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        if (active >= 0) activate(active);
        break;
      case "Escape":
        e.preventDefault();
        onclose(level > 0 ? "left" : "escape");
        break;
      case "Tab":
        e.preventDefault();
        onclose("tab");
        break;
      default:
        if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) {
          const ch = e.key.toLowerCase();
          const n = items.length;
          for (let k = 1; k <= n; k++) {
            const i = (active + k + n) % n;
            const it = items[i];
            if (enabled(i) && isItem(it) && it.label.toLowerCase().startsWith(ch)) {
              focusItem(i);
              break;
            }
          }
        }
    }
  }

  function hover(i: number) {
    clearTimeout(hoverTimer);
    const it = items[i];
    if (!isItem(it)) return;
    active = i;
    itemEls().find((n) => Number(n.dataset.index) === i)?.focus({ preventScroll: true });
    if (sub && sub.index !== i) {
      hoverTimer = window.setTimeout(() => (sub = null), 180);
    }
    if (it.submenu && !it.disabled && sub?.index !== i) {
      hoverTimer = window.setTimeout(() => openSub(i, false), 120);
    }
  }

  async function position() {
    await tick();
    if (!el) return;
    const r = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = x;
    let top = y;
    if (anchor && placement === "below") {
      left = anchor.left;
      top = anchor.bottom + 2;
      if (top + r.height > vh - 4) top = Math.max(4, anchor.top - r.height - 2);
    } else if (anchor && placement === "right") {
      left = anchor.right - 2;
      top = anchor.top - 5;
      if (left + r.width > vw - 4) left = anchor.left - r.width + 2;
    }
    left = Math.max(4, Math.min(vw - r.width - 4, left));
    top = Math.max(4, Math.min(vh - r.height - 4, top));
    pos = { left, top };
  }

  onMount(() => {
    position();
    if (autofocus) queueMicrotask(() => step(1));
    else el.focus({ preventScroll: true });
    if (level > 0) return;
    const outside = (e: PointerEvent) => {
      const t = e.target as Node;
      if ((t as Element).closest?.(".ops-menu")) return;
      if (ignore && ignore.contains(t)) return;
      onclose("outside");
    };
    const blur = () => onclose("outside");
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", blur);
    window.addEventListener("resize", blur);
    return () => {
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("blur", blur);
      window.removeEventListener("resize", blur);
      clearTimeout(hoverTimer);
    };
  });

  const subEntries = $derived.by(() => {
    if (!sub) return null;
    const it = items[sub.index];
    return it && isItem(it) ? subItems(it as MenuItem) : null;
  });
</script>

<div
  class="ops-menu"
  bind:this={el}
  use:portal
  role="menu"
  tabindex="-1"
  aria-label={label}
  data-testid={testid}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  style:min-width="{minWidth}px"
  onkeydown={onKey}
>
  <div class="ops-menu__list">
    {#each items as entry, i (i)}
      {#if entry.type === "separator"}
        <div class="sep" role="separator"></div>
      {:else if entry.type === "heading"}
        <div class="heading" role="presentation">{entry.label}</div>
      {:else}
        {@const it = entry as MenuItem}
        <div
          class="item"
          class:active={active === i}
          class:disabled={it.disabled}
          class:open={sub?.index === i}
          data-index={i}
          data-testid={it.testid}
          role={it.checked !== undefined ? (it.radio ? "menuitemradio" : "menuitemcheckbox") : "menuitem"}
          aria-checked={it.checked !== undefined ? it.checked : undefined}
          aria-disabled={it.disabled || undefined}
          aria-haspopup={it.submenu ? "menu" : undefined}
          aria-expanded={it.submenu ? sub?.index === i : undefined}
          tabindex="-1"
          use:tooltip={it.disabled && it.hint ? { text: it.hint, placement: "right" } : null}
          onpointerenter={() => hover(i)}
          onclick={() => activate(i)}
        >
          <span class="check">
            {#if it.checked}
              {#if it.radio}<span class="dot"></span>{:else}<Check size={12} strokeWidth={2.25} />{/if}
            {/if}
          </span>
          {#if it.icon}
            {@const Icon = it.icon}
            <span class="icon"><Icon size={14} /></span>
          {/if}
          <span class="label">{it.label}</span>
          {#if it.shortcut}<span class="key">{formatShortcut(it.shortcut)}</span>{/if}
          {#if it.submenu}<span class="chev"><ChevronRight size={12} /></span>{/if}
        </div>
      {/if}
    {/each}
  </div>
  {#if sub && subEntries}
    <Menu
      items={subEntries}
      anchor={sub.rect}
      placement="right"
      level={level + 1}
      autofocus={sub.focus}
      onclose={(reason) => {
        const idx = sub?.index ?? -1;
        sub = null;
        if (reason === "left") queueMicrotask(() => focusItem(idx));
        else onclose(reason);
      }}
    />
  {/if}
</div>

<style>
  .ops-menu {
    position: fixed;
    z-index: 200;
    max-height: calc(100vh - 8px);
    overflow-y: auto;
    padding: 4px;
    background: var(--surface-raised);
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-lg);
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-strong);
    user-select: none;
    outline: none;
  }
  .ops-menu:focus-visible {
    box-shadow: var(--shadow-lg);
  }
  .item {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 8px 0 2px;
    border-radius: var(--radius-xs);
    cursor: default;
    outline: none;
    white-space: nowrap;
  }
  .item.active,
  .item.open {
    background: var(--surface-active);
  }
  .item.disabled {
    color: var(--text-faint);
  }
  .item.disabled.active {
    background: transparent;
  }
  .check {
    display: grid;
    place-items: center;
    width: 18px;
    flex: 0 0 auto;
    color: var(--text-strong);
  }
  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 18px;
    margin-right: 6px;
    color: var(--text-muted);
  }
  .label {
    flex: 1;
    padding-right: var(--space-6);
  }
  .key {
    font-family: var(--font-sans);
    color: var(--text-faint);
    letter-spacing: 0.04em;
  }
  .item.disabled .key {
    opacity: 0.7;
  }
  .chev {
    display: grid;
    place-items: center;
    margin-left: var(--space-2);
    margin-right: -4px;
    color: var(--text-muted);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border-hairline);
  }
  .heading {
    padding: 8px 8px 4px 20px;
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-sans);
    letter-spacing: var(--tracking-caps);
    text-transform: uppercase;
    color: var(--text-faint);
  }
</style>
