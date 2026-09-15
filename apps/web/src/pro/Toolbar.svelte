<script lang="ts">
  // The single-column toolbar. Each button shows its group's current tool; a
  // corner mark means more tools are nested — long-press or right-click for
  // the flyout. Below: foreground/background colours, swap (X), default (D)
  // and Quick Mask (Q).
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import SquareDashed from "@lucide/svelte/icons/square-dashed";
  import Square from "@lucide/svelte/icons/square";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import Menu from "../ui/Menu.svelte";
  import type { MenuEntry } from "../ui/menu";
  import { paintTarget } from "../ui/paint-target.svelte";
  import { css } from "../ui/color";
  import { tooltip } from "../ui/tooltip";
  import { pro } from "./state.svelte";
  import { resetColors as reset, swapColors as swap } from "./colors";
  import { isToolAvailable, selectTool, toolBridge, toolMeta, type ToolGroup } from "./tools.svelte";

  let flyout = $state<{ group: ToolGroup; rect: DOMRect; focus: boolean } | null>(null);
  let pressTimer = 0;

  function shown(g: ToolGroup) {
    if (g.tools.includes(editor.tool)) return editor.tool;
    return toolBridge.current[g.id] ?? g.tools[0];
  }

  function openFlyout(g: ToolGroup, el: HTMLElement, focus = false) {
    if (g.tools.length < 2) return;
    flyout = { group: g, rect: el.getBoundingClientRect(), focus };
  }

  function items(g: ToolGroup): MenuEntry[] {
    return g.tools.map((id) => {
      const m = toolMeta(id);
      return {
        label: m.label,
        icon: m.icon,
        shortcut: m.shortcut,
        checked: editor.tool === id,
        radio: true,
        testid: `tool-${id}-flyout`,
        run: () => selectTool(id),
      };
    });
  }

</script>

<nav class="toolbar" aria-label={t("Tools")} data-testid="toolbar">
  <div class="tools" role="toolbar" aria-orientation="vertical" aria-label={t("Tools")}>
    {#each toolBridge.groups as g, i (g.id)}
      {@const id = shown(g)}
      {@const m = toolMeta(id)}
      {@const Icon = m.icon}
      {#if i > 0 && ["crop", "spot-heal", "heal", "text", "hand"].includes(g.id)}<span class="sep" role="separator"></span>{/if}
      <button
        type="button"
        class="tool"
        class:active={g.tools.includes(editor.tool)}
        class:unavailable={!isToolAvailable(id)}
        aria-label={m.label}
        aria-pressed={g.tools.includes(editor.tool)}
        aria-haspopup={g.tools.length > 1 ? "menu" : undefined}
        data-testid="tool-{id}"
        data-group={g.id}
        use:tooltip={{ text: isToolAvailable(id) ? m.label : t("{tool} (not available yet)", { tool: m.label }), shortcut: m.shortcut, placement: "right" }}
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          const el = e.currentTarget as HTMLElement;
          clearTimeout(pressTimer);
          pressTimer = window.setTimeout(() => openFlyout(g, el), 380);
        }}
        onpointerup={() => clearTimeout(pressTimer)}
        onpointerleave={() => clearTimeout(pressTimer)}
        onclick={() => {
          if (!flyout) selectTool(id);
        }}
        oncontextmenu={(e) => {
          e.preventDefault();
          openFlyout(g, e.currentTarget as HTMLElement);
        }}
        onkeydown={(e) => {
          if (e.key === "ArrowRight" && g.tools.length > 1) {
            e.preventDefault();
            openFlyout(g, e.currentTarget as HTMLElement, true);
          }
        }}
      >
        <Icon size={16} strokeWidth={1.75} />
        {#if g.tools.length > 1}<span class="corner" aria-hidden="true"></span>{/if}
      </button>
    {/each}
  </div>

  <div class="colors">
    <div class="swatches">
      <button
        type="button"
        class="chip bg"
        style:background={css(editor.secondary)}
        aria-label={t("Background colour")}
        data-testid="bg-color"
        use:tooltip={{ text: t("Set background colour"), placement: "right" }}
        onclick={() => pro.open({ kind: "color", which: "secondary" })}
      ></button>
      <button
        type="button"
        class="chip fg"
        style:background={css(editor.primary)}
        aria-label={t("Foreground colour")}
        data-testid="fg-color"
        use:tooltip={{ text: t("Set foreground colour"), placement: "right" }}
        onclick={() => pro.open({ kind: "color", which: "primary" })}
      ></button>
      <button type="button" class="mini swap" aria-label={t("Switch colours")} use:tooltip={{ text: t("Switch colours"), shortcut: "X", placement: "right" }} onclick={swap}>
        <ArrowLeftRight size={10} />
      </button>
      <button type="button" class="mini reset" aria-label={t("Default colours")} use:tooltip={{ text: t("Default colours"), shortcut: "D", placement: "right" }} onclick={reset}>
        <span class="d-fg"></span><span class="d-bg"></span>
      </button>
    </div>
    <button
      type="button"
      class="tool qm"
      class:active={paintTarget.quickMask}
      aria-pressed={paintTarget.quickMask}
      aria-label={t("Edit in Quick Mask mode")}
      data-testid="quick-mask"
      use:tooltip={{ text: t("Edit in Quick Mask mode"), shortcut: "Q", placement: "right" }}
      onclick={() => (paintTarget.quickMask = !paintTarget.quickMask)}
    >
      {#if paintTarget.quickMask}<SquareDashed size={16} strokeWidth={1.75} />{:else}<Square size={16} strokeWidth={1.75} />{/if}
    </button>
  </div>
</nav>

{#if flyout}
  <Menu
    items={items(flyout.group)}
    anchor={flyout.rect}
    placement="right"
    autofocus={flyout.focus}
    label={t("Tools")}
    onclose={() => (flyout = null)}
  />
{/if}

<style>
  .toolbar {
    width: 40px;
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
    padding: 6px 0;
    background: var(--bg-page);
    border-right: var(--border-width) solid var(--border-hairline);
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: none;
  }
  .tools {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .sep {
    width: 20px;
    height: 1px;
    margin: 4px 0;
    background: var(--border-hairline);
  }
  .tool {
    position: relative;
    display: grid;
    place-items: center;
    width: 30px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    cursor: default;
    transition: var(--transition-control);
  }
  .tool:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }
  .tool.active {
    color: var(--text-strong);
    background: var(--surface-active);
  }
  .tool.unavailable:not(.active) {
    color: var(--text-faint);
  }
  .corner {
    position: absolute;
    right: 3px;
    bottom: 3px;
    width: 0;
    height: 0;
    border-left: 3px solid transparent;
    border-bottom: 3px solid currentColor;
    opacity: 0.7;
  }
  .colors {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding-top: 8px;
  }
  .swatches {
    position: relative;
    width: 32px;
    height: 32px;
  }
  .chip {
    position: absolute;
    width: 18px;
    height: 18px;
    padding: 0;
    border-radius: 2px;
    border: 1px solid var(--border-strong);
    box-shadow: 0 0 0 1px var(--bg-page);
    cursor: default;
  }
  .fg {
    left: 2px;
    top: 2px;
    z-index: 1;
  }
  .bg {
    right: 2px;
    bottom: 2px;
  }
  .mini {
    position: absolute;
    display: grid;
    place-items: center;
    width: 11px;
    height: 11px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: default;
  }
  .mini:hover {
    color: var(--text-strong);
  }
  .swap {
    right: -1px;
    top: -1px;
  }
  .reset {
    left: -1px;
    bottom: -1px;
  }
  .d-fg,
  .d-bg {
    position: absolute;
    width: 6px;
    height: 6px;
    border: 1px solid var(--text-muted);
  }
  .d-fg {
    left: 1px;
    top: 1px;
    background: #000;
    z-index: 1;
  }
  .d-bg {
    right: 1px;
    bottom: 1px;
    background: #fff;
  }
</style>
