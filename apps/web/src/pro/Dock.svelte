<script lang="ts">
  // The right dock: tabbed panel groups (Color/Swatches, Properties/
  // Adjustments, Layers), an icon strip for secondary groups that open as
  // flyouts (Navigator/Histogram/Info, History), a draggable left edge for
  // width, a splitter between Properties and Layers, and double-click on a
  // tab strip to collapse its group.
  import type { Component } from "svelte";
  import Compass from "@lucide/svelte/icons/compass";
  import History from "@lucide/svelte/icons/history";
  import Palette from "@lucide/svelte/icons/palette";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Layers from "@lucide/svelte/icons/layers";
  import ChevronsRight from "@lucide/svelte/icons/chevrons-right";
  import ChevronsLeft from "@lucide/svelte/icons/chevrons-left";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { t } from "../lib/i18n";
  import IconButton from "../ui/IconButton.svelte";
  import Tabs from "../ui/Tabs.svelte";
  import { MAIN_GROUPS, PANEL_GROUPS, SECONDARY_GROUPS, pro, type PanelId } from "./state.svelte";
  import LayersPanel from "./panels/LayersPanel.svelte";
  import PropertiesPanel from "./panels/PropertiesPanel.svelte";
  import AdjustmentsPanel from "./panels/AdjustmentsPanel.svelte";
  import HistoryPanel from "./panels/HistoryPanel.svelte";
  import ColorPanel from "./panels/ColorPanel.svelte";
  import SwatchesPanel from "./panels/SwatchesPanel.svelte";
  import HistogramPanel from "./panels/HistogramPanel.svelte";
  import InfoPanel from "./panels/InfoPanel.svelte";
  import NavigatorPanel from "./panels/NavigatorPanel.svelte";

  const PANELS: Record<PanelId, { label: string; component: Component }> = {
    layers: { label: t("Layers"), component: LayersPanel },
    properties: { label: t("Properties"), component: PropertiesPanel },
    adjustments: { label: t("Adjustments"), component: AdjustmentsPanel },
    history: { label: t("History"), component: HistoryPanel },
    color: { label: t("Color"), component: ColorPanel },
    swatches: { label: t("Swatches"), component: SwatchesPanel },
    histogram: { label: t("Histogram"), component: HistogramPanel },
    info: { label: t("Info"), component: InfoPanel },
    navigator: { label: t("Navigator"), component: NavigatorPanel },
  };
  const GROUP_ICONS: Record<string, Component<{ size?: number | string }>> = { nav: Compass, history: History, color: Palette, props: SlidersHorizontal, layers: Layers };

  function members(g: string) {
    return PANEL_GROUPS[g].filter((p) => pro.layout.visible[p]);
  }
  function current(g: string): PanelId | null {
    const m = members(g);
    if (!m.length) return null;
    const want = pro.layout.groupTab[g];
    return m.includes(want) ? want : m[0];
  }

  const mainGroups = $derived(MAIN_GROUPS.filter((g) => members(g).length));
  const collapsed = (g: string) => pro.layout.collapsedGroups.includes(g);

  function toggleCollapse(g: string) {
    pro.layout.collapsedGroups = collapsed(g) ? pro.layout.collapsedGroups.filter((x) => x !== g) : [...pro.layout.collapsedGroups, g];
    pro.save();
  }

  // Width drag on the dock's left edge.
  let resize: { x: number; w: number } | null = null;
  function edgeDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    resize = { x: e.clientX, w: pro.layout.dockWidth };
  }
  function edgeMove(e: PointerEvent) {
    if (!resize) return;
    pro.layout.dockWidth = Math.round(Math.min(560, Math.max(240, resize.w + resize.x - e.clientX)));
  }
  function edgeUp() {
    if (resize) pro.save();
    resize = null;
  }

  // Splitter between the Properties group and Layers.
  let split: { y: number; h: number } | null = null;
  function splitDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    split = { y: e.clientY, h: pro.layout.propsHeight };
  }
  function splitMove(e: PointerEvent) {
    if (!split) return;
    pro.layout.propsHeight = Math.round(Math.min(window.innerHeight - 260, Math.max(120, split.h + e.clientY - split.y)));
  }
  function splitUp() {
    if (split) pro.save();
    split = null;
  }
</script>

<div class="dock-wrap" data-testid="dock">
  <div class="strip" role="toolbar" aria-orientation="vertical" aria-label={t("Panels")}>
    {#if pro.layout.dockCollapsed}
      {#each MAIN_GROUPS as g (g)}
        {@const Icon = GROUP_ICONS[g]}
        <IconButton label={members(g).map((p) => PANELS[p].label).join(", ")} placement="left" selected={pro.flyout === g} onclick={() => (pro.flyout = pro.flyout === g ? null : g)}>
          <Icon size={15} />
        </IconButton>
      {/each}
      <span class="strip-sep"></span>
    {/if}
    {#each SECONDARY_GROUPS as g (g)}
      {#if members(g).length}
        {@const Icon = GROUP_ICONS[g]}
        <IconButton
          label={members(g).map((p) => PANELS[p].label).join(", ")}
          placement="left"
          selected={pro.flyout === g}
          testid="strip-{g}"
          onclick={() => (pro.flyout = pro.flyout === g ? null : g)}
        >
          <Icon size={15} />
        </IconButton>
      {/if}
    {/each}
    <span class="grow"></span>
    <IconButton
      label={pro.layout.dockCollapsed ? t("Expand panels") : t("Collapse panels to icons")}
      placement="left"
      onclick={() => {
        pro.layout.dockCollapsed = !pro.layout.dockCollapsed;
        pro.flyout = null;
        pro.save();
      }}
    >
      {#if pro.layout.dockCollapsed}<ChevronsLeft size={14} />{:else}<ChevronsRight size={14} />{/if}
    </IconButton>
  </div>

  {#if pro.flyout && members(pro.flyout).length}
    {@const g = pro.flyout}
    {@const cur = current(g)}
    <section class="flyout" aria-label={members(g).map((p) => PANELS[p].label).join(", ")} data-testid="flyout-{g}">
      <Tabs
        tabs={members(g).map((p) => ({ id: p, label: PANELS[p].label }))}
        active={cur ?? ""}
        label={t("Panels")}
        onselect={(id) => {
          pro.layout.groupTab[g] = id as PanelId;
          pro.save();
        }}
      >
        {#snippet trailing()}
          <IconButton size="xs" label={t("Close")} onclick={() => (pro.flyout = null)}><ChevronsRight size={12} /></IconButton>
        {/snippet}
      </Tabs>
      <div class="body" class:tall={g === "history"}>
        {#if cur}
          {@const P = PANELS[cur].component}
          <P />
        {/if}
      </div>
    </section>
  {/if}

  {#if !pro.layout.dockCollapsed}
    <aside class="dock" style:width="{pro.layout.dockWidth}px" aria-label={t("Panels")}>
      <div class="edge" role="separator" aria-orientation="vertical" aria-label={t("Resize panels")} onpointerdown={edgeDown} onpointermove={edgeMove} onpointerup={edgeUp}></div>
      {#each mainGroups as g (g)}
        {@const cur = current(g)}
        {@const isCollapsed = collapsed(g)}
        <section
          class="group group-{g}"
          class:collapsed={isCollapsed}
          style:height={g === "props" && !isCollapsed && mainGroups.includes("layers") ? `max(120px, min(${pro.layout.propsHeight}px, calc(100% - 420px)))` : undefined}
          data-testid="group-{g}"
        >
          <Tabs
            tabs={members(g).map((p) => ({ id: p, label: PANELS[p].label }))}
            active={cur ?? ""}
            label={t("Panels")}
            ondblclick={() => toggleCollapse(g)}
            onselect={(id) => {
              pro.layout.groupTab[g] = id as PanelId;
              if (isCollapsed) toggleCollapse(g);
              pro.save();
            }}
          >
            {#snippet trailing()}
              <IconButton size="xs" label={isCollapsed ? t("Expand") : t("Collapse")} onclick={() => toggleCollapse(g)}>
                {#if isCollapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
              </IconButton>
            {/snippet}
          </Tabs>
          {#if !isCollapsed && cur}
            {@const P = PANELS[cur].component}
            <div class="body">
              <P />
            </div>
          {/if}
        </section>
        {#if g === "props" && !isCollapsed && mainGroups.includes("layers")}
          <div class="split" role="separator" aria-orientation="horizontal" aria-label={t("Resize")} onpointerdown={splitDown} onpointermove={splitMove} onpointerup={splitUp}></div>
        {/if}
      {/each}
    </aside>
  {/if}
</div>

<style>
  .dock-wrap {
    position: relative;
    display: flex;
    flex: 0 0 auto;
    min-height: 0;
    height: 100%;
  }
  .strip {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    width: 36px;
    padding: 6px 0;
    background: var(--bg-page);
    border-left: var(--border-width) solid var(--border-hairline);
  }
  .strip-sep {
    width: 18px;
    height: 1px;
    margin: 4px 0;
    background: var(--border-hairline);
  }
  .grow {
    flex: 1;
  }
  .dock {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-subtle);
    border-left: var(--border-width) solid var(--border-hairline);
  }
  .edge {
    position: absolute;
    left: -3px;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    z-index: 2;
    touch-action: none;
  }
  .group {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 0 0 auto;
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .group.collapsed {
    height: auto !important;
  }
  .group-layers:not(.collapsed) {
    flex: 1 1 0;
    min-height: 160px;
  }
  .group-props {
    min-height: 0;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .group-layers .body {
    overflow: hidden;
  }
  .split {
    height: 5px;
    margin: -3px 0 -2px;
    cursor: row-resize;
    position: relative;
    z-index: 2;
    touch-action: none;
  }
  .flyout {
    position: absolute;
    right: calc(100% + 0px);
    top: 0;
    width: 264px;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-subtle);
    border: var(--border-width) solid var(--border-strong);
    border-right: 0;
    box-shadow: var(--shadow-lg);
    z-index: 20;
  }
  .flyout .body.tall {
    height: 360px;
  }
</style>
