<script lang="ts">
  // An application menu bar. Click a title to open it; while one is open,
  // hovering another switches. Left and Right move between menus, Down or
  // Enter opens with the first item focused. Items are built when a menu
  // opens, so enabled and checked states are always current.
  import Menu from "./Menu.svelte";
  import type { MenuBarMenu, MenuEntry } from "./menu";

  let { menus, label = "Menu", onopenchange }: { menus: MenuBarMenu[]; label?: string; onopenchange?: (open: boolean) => void } = $props();

  let bar = $state<HTMLDivElement>() as HTMLDivElement;
  let open = $state<{ index: number; items: MenuEntry[]; rect: DOMRect; focus: boolean } | null>(null);
  let focusIndex = $state(0);

  function buttons() {
    return [...bar.querySelectorAll<HTMLButtonElement>("button[data-menu]")];
  }

  function show(index: number, focus: boolean) {
    const btn = buttons()[index];
    if (!btn) return;
    focusIndex = index;
    open = { index, items: menus[index].items(), rect: btn.getBoundingClientRect(), focus };
    onopenchange?.(true);
  }

  function close(returnFocus: boolean) {
    const idx = open?.index ?? focusIndex;
    open = null;
    onopenchange?.(false);
    if (returnFocus) buttons()[idx]?.focus();
  }

  function onBarKey(e: KeyboardEvent, i: number) {
    const n = menus.length;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const j = (i + (e.key === "ArrowRight" ? 1 : -1) + n) % n;
      focusIndex = j;
      buttons()[j]?.focus();
    } else if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      show(i, true);
    } else if (e.key === "Escape") {
      (e.currentTarget as HTMLElement).blur();
    }
  }
</script>

<div class="ops-menubar" role="menubar" aria-label={label} bind:this={bar}>
  {#each menus as m, i (m.id)}
    <button
      type="button"
      class="ops-menubar__title"
      class:open={open?.index === i}
      data-menu={m.id}
      data-testid="menu-{m.id}"
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={open?.index === i}
      tabindex={focusIndex === i ? 0 : -1}
      onpointerdown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        if (open?.index === i) close(false);
        else show(i, false);
      }}
      onpointerenter={() => {
        if (open && open.index !== i) show(i, false);
      }}
      onkeydown={(e) => onBarKey(e, i)}
    >
      {m.label}
    </button>
  {/each}
</div>

{#if open}
  {#key open.index}
    <Menu
      items={open.items}
      anchor={open.rect}
      placement="below"
      autofocus={open.focus}
      ignore={bar}
      label={menus[open.index].label}
      testid="menu-{menus[open.index].id}-popup"
      onclose={(reason) => close(reason === "escape")}
      onnavigate={(dir) => {
        const n = menus.length;
        show(((open?.index ?? 0) + dir + n) % n, true);
      }}
    />
  {/key}
{/if}

<style>
  .ops-menubar {
    display: flex;
    align-items: stretch;
    height: 100%;
  }
  .ops-menubar__title {
    display: inline-flex;
    align-items: center;
    padding: 0 8px;
    margin: 3px 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-body);
    cursor: default;
    user-select: none;
  }
  .ops-menubar__title:hover,
  .ops-menubar__title.open {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .ops-menubar__title:focus-visible {
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }
</style>
