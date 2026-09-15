<script lang="ts">
  // The Lite profile: simple, fast fixes. Desktop is canvas in the middle,
  // categories on the left, controls on the right; a phone is canvas on top,
  // a sheet of controls, and a tab bar. Everything drives the same engine
  // document Pro edits, so "Open in Pro" loses nothing.
  import "./lite.css";
  import { onMount, untrack } from "svelte";
  import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Crop from "@lucide/svelte/icons/crop";
  import Bandage from "@lucide/svelte/icons/bandage";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Blend from "@lucide/svelte/icons/blend";
  import Zap from "@lucide/svelte/icons/zap";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import Redo2 from "@lucide/svelte/icons/redo-2";
  import History from "@lucide/svelte/icons/history";
  import Search from "@lucide/svelte/icons/search";
  import Layers from "@lucide/svelte/icons/layers";
  import Download from "@lucide/svelte/icons/download";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import X from "@lucide/svelte/icons/x";
  import SquareSplitHorizontal from "@lucide/svelte/icons/square-split-horizontal";
  import ImagePlus from "@lucide/svelte/icons/image-plus";
  import Maximize from "@lucide/svelte/icons/maximize";
  import Canvas from "../canvas/Canvas.svelte";
  import BrandMark from "../ui/BrandMark.svelte";
  import Menu from "../ui/Menu.svelte";
  import type { MenuEntry } from "../ui/menu";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { textEdit, commitText } from "../tools/text.svelte";
  import { CATEGORY_LABELS, openCategory, openPhoto } from "./actions";
  import { lite, type Category } from "./lite.svelte";
  import { activatePixels, activateTop, liteTools } from "./tools.svelte";
  import StartScreen from "./StartScreen.svelte";
  import PromptBar from "./PromptBar.svelte";
  import StepsPanel from "./StepsPanel.svelte";
  import ExportSheet from "./ExportSheet.svelte";
  import ToolSearch from "./ToolSearch.svelte";
  import AutoPanel from "./panels/AutoPanel.svelte";
  import AdjustPanel from "./panels/AdjustPanel.svelte";
  import CropPanel from "./panels/CropPanel.svelte";
  import RetouchPanel from "./panels/RetouchPanel.svelte";
  import MarkupPanel from "./panels/MarkupPanel.svelte";
  import EffectsPanel from "./panels/EffectsPanel.svelte";
  import EnhancePanel from "./panels/EnhancePanel.svelte";

  const CATEGORIES: { id: Category; icon: typeof Crop; blurb: string }[] = [
    { id: "auto", icon: WandSparkles, blurb: "Pick the version you like" },
    { id: "adjust", icon: SlidersHorizontal, blurb: "Light, color and detail" },
    { id: "crop", icon: Crop, blurb: "Frame, straighten and resize" },
    { id: "retouch", icon: Bandage, blurb: "Erase things, heal spots, fix red eye" },
    { id: "markup", icon: PenLine, blurb: "Arrows, boxes, text and redaction" },
    { id: "effects", icon: Blend, blurb: "Looks, finishing touches and frames" },
    { id: "enhance", icon: Zap, blurb: "Bigger, cleaner and sharper" },
  ];

  const isMac = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
  const mod = isMac ? "⌘" : "Ctrl+";

  let stage = $state<HTMLDivElement>();
  let menuAnchor = $state<DOMRect | null>(null);
  let menuButton = $state<HTMLButtonElement>();
  const current = $derived(CATEGORIES.find((c) => c.id === lite.category)!);
  const docSize = $derived(editor.summary && editor.hasDocument ? `${editor.summary.width} × ${editor.summary.height}` : "");

  // ---------------------------------------------------------------------
  // Tools follow the category.

  $effect(() => {
    const c = lite.category;
    if (!editor.hasDocument) return;
    untrack(() => {
      liteTools.sync();
      const id = liteTools.toolFor(c);
      if (editor.tool !== id) editor.tool = id;
      if (c === "markup") void activateTop();
      if (c === "retouch") void activatePixels();
    });
  });

  // Keep the sheet's bookkeeping honest as history moves.
  $effect(() => {
    void editor.summary?.history.redo.length;
    untrack(() => lite.syncPhantom());
  });

  // Refit when the photo's size changes (crop, rotate, border) or the stage resizes.
  $effect(() => {
    void editor.summary?.width;
    void editor.summary?.height;
    void editor.hasDocument;
    untrack(() => requestAnimationFrame(() => editor.fit()));
  });

  onMount(() => {
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKeyUp, true);
      void editor.setPreviewHidden([]);
    };
  });

  $effect(() => {
    if (stage) {
      const ro = new ResizeObserver(() => requestAnimationFrame(() => editor.hasDocument && editor.fit()));
      ro.observe(stage);
      return () => ro.disconnect();
    }
  });

  // ---------------------------------------------------------------------
  // Before and after: hide Lite's adjustments and markup in the viewport only.

  async function compare(on: boolean) {
    if (!editor.hasDocument || lite.comparing === on) return;
    lite.comparing = on;
    const ids = on ? lite.layers().filter((l) => (l.kind === "adjustment" || l.kind === "shape" || l.kind === "text") && l.visible).map((l) => l.id) : [];
    await editor.setPreviewHidden(ids);
  }

  // ---------------------------------------------------------------------
  // Keyboard

  function typing(el: EventTarget | null) {
    const e = el as HTMLElement | null;
    return !!e && (e.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(e.tagName));
  }

  function onKey(e: KeyboardEvent) {
    const modKey = isMac ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    if (modKey && k === "k") {
      e.preventDefault();
      lite.searchOpen = !lite.searchOpen;
      return;
    }
    if (modKey && k === "o") {
      e.preventDefault();
      void openPhoto();
      return;
    }
    if (!editor.hasDocument) return;
    if (modKey && (k === "s" || k === "e")) {
      e.preventDefault();
      lite.exportOpen = true;
      return;
    }
    if (typing(e.target)) return;
    if (modKey && k === "z") {
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) void lite.redo();
      else void lite.undo();
      return;
    }
    if (modKey && k === "y") {
      e.preventDefault();
      void lite.redo();
      return;
    }
    if (e.key === "Escape") {
      if (lite.menuOpen) lite.menuOpen = false;
      else if (lite.exportOpen) lite.exportOpen = false;
      else if (lite.stepsOpen) lite.stepsOpen = false;
      return;
    }
    if (e.key === "\\" && !e.repeat) void compare(true);
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === "\\") void compare(false);
  }

  // ---------------------------------------------------------------------

  function selectCategory(c: Category) {
    if (textEdit.session) void commitText(editor);
    if (lite.phone && lite.category === c && !lite.stepsOpen) {
      lite.sheetOpen = !lite.sheetOpen;
    } else {
      openCategory(c);
    }
    lite.stepsOpen = false;
  }

  const menuItems = $derived<MenuEntry[]>([
    { label: t("Steps"), icon: History, run: () => { lite.stepsOpen = true; lite.sheetOpen = true; }, testid: "menu-steps" },
    { label: t("Search tools"), icon: Search, shortcut: "Mod+K", run: () => (lite.searchOpen = true) },
    { label: t("Fit to screen"), icon: Maximize, run: () => editor.fit() },
    { type: "separator" },
    { label: t("Open another photo"), icon: ImagePlus, shortcut: "Mod+O", run: () => openAnother() },
    { label: t("Open in Pro"), icon: Layers, run: () => editor.setProfile("pro") },
  ]);

  function openAnother() {
    if (editor.dirty && !confirm(t("Open another photo? Changes to this one that you have not exported will be lost."))) return;
    void openPhoto();
  }
</script>

{#if !editor.hasDocument}
  <StartScreen />
{:else}
  <div class="lite" class:phone={lite.phone} data-testid="lite">
    <header class="bar">
      <div class="bar__start">
        <span class="brand"><BrandMark variant={lite.phone ? "monogram" : "wordmark"} size={lite.phone ? 28 : 18} /></span>
        {#if !lite.phone}
          <span class="sep" aria-hidden="true"></span>
          <span class="file" title={editor.fileName}>
            <span class="name">{editor.fileName}</span>
            <span class="dims">{docSize}</span>
          </span>
        {/if}
      </div>

      <div class="bar__mid">
        <button class="oa-icon-btn" aria-label={t("Undo")} title={t("Undo ({key})", { key: `${mod}Z` })} disabled={!lite.undoCount} data-testid="undo" onclick={() => lite.undo()}><Undo2 size={18} /></button>
        <button class="oa-icon-btn" aria-label={t("Redo")} title={t("Redo ({key})", { key: `${mod}⇧Z` })} disabled={!lite.redoCount} data-testid="redo" onclick={() => lite.redo()}><Redo2 size={18} /></button>
        <button
          class="compare"
          class:on={lite.comparing}
          aria-label={t("Hold to see the original")}
          title={t("Hold to see the original (\\)")}
          data-testid="compare"
          disabled={!lite.undoCount}
          onpointerdown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            void compare(true);
          }}
          onpointerup={() => compare(false)}
          onpointercancel={() => compare(false)}
          onlostpointercapture={() => compare(false)}
          oncontextmenu={(e) => e.preventDefault()}
        >
          <SquareSplitHorizontal size={18} />
          {#if !lite.phone}<span>{t("Compare")}</span>{/if}
        </button>
      </div>

      <div class="bar__end">
        {#if lite.phone}
          <button class="oa-icon-btn" aria-label={t("More")} bind:this={menuButton} data-testid="more" onclick={() => { menuAnchor = menuButton!.getBoundingClientRect(); lite.menuOpen = true; }}><Ellipsis size={20} /></button>
        {:else}
          <button class="oa-icon-btn" class:oa-icon-btn--selected={lite.stepsOpen} aria-label={t("Steps")} title={t("Steps")} aria-pressed={lite.stepsOpen} data-testid="steps-toggle" onclick={() => (lite.stepsOpen = !lite.stepsOpen)}><History size={18} /></button>
          <button class="search" data-testid="search" onclick={() => (lite.searchOpen = true)}>
            <Search size={15} />
            <span>{t("Search")}</span>
            <kbd>{mod}K</kbd>
          </button>
          <span class="sep" aria-hidden="true"></span>
          <button class="oa-btn oa-btn--ghost oa-btn--md" data-testid="open-pro" onclick={() => editor.setProfile("pro")}><Layers size={16} />{t("Open in Pro")}</button>
        {/if}
        <button class="oa-btn oa-btn--primary oa-btn--md export" data-testid="export" onclick={() => (lite.exportOpen = true)}>
          <Download size={16} />{t("Export")}
        </button>
      </div>
    </header>

    <div class="body">
      {#if !lite.phone}
        <nav class="rail" aria-label={t("Categories")}>
          {#each CATEGORIES as c (c.id)}
            {@const Icon = c.icon}
            <button class="rail__btn" aria-current={lite.category === c.id && !lite.stepsOpen ? "page" : undefined} data-testid="cat-{c.id}" onclick={() => selectCategory(c.id)}>
              <span class="rail__icon"><Icon size={20} /></span>
              <span class="rail__label">{t(CATEGORY_LABELS[c.id])}</span>
            </button>
          {/each}
        </nav>
      {/if}

      <div class="stage" bind:this={stage}>
        <div class="canvas" style:transform={lite.straightenPreview ? `rotate(${lite.straightenPreview}deg)` : undefined}>
          <Canvas />
        </div>
        {#if lite.straightenPreview}
          <div class="grid" aria-hidden="true"></div>
        {/if}
        {#if lite.comparing}
          <span class="badge" data-testid="compare-badge">{t("Original")}</span>
        {/if}
        {#if !lite.phone}
          <div class="float">
            <PromptBar />
          </div>
        {/if}
      </div>

      {#if lite.phone}
        <section class="sheet" class:collapsed={!lite.sheetOpen} aria-label={lite.stepsOpen ? t("Steps") : t(CATEGORY_LABELS[lite.category])}>
          <button
            class="sheet__head"
            aria-expanded={lite.sheetOpen}
            data-testid="sheet-head"
            onclick={() => {
              if (lite.stepsOpen) lite.stepsOpen = false;
              else lite.sheetOpen = !lite.sheetOpen;
            }}
          >
            <span class="sheet__title">{lite.stepsOpen ? t("Steps") : t(CATEGORY_LABELS[lite.category])}</span>
            <span class="sheet__blurb">{lite.stepsOpen ? "" : t(current.blurb)}</span>
            {#if lite.stepsOpen}
              <span class="chev"><X size={16} /></span>
            {:else}
              <span class="chev" class:down={lite.sheetOpen}><ChevronDown size={18} /></span>
            {/if}
          </button>
          {#if lite.sheetOpen}
            <div class="sheet__body">
              {#if lite.stepsOpen}
                <StepsPanel />
              {:else}
                {#if lite.category === "auto" || lite.category === "adjust"}
                  <div class="sheet__prompt"><PromptBar compact /></div>
                {/if}
                {@render panel()}
              {/if}
            </div>
          {/if}
        </section>
        <nav class="tabs" aria-label={t("Categories")}>
          {#each CATEGORIES as c (c.id)}
            {@const Icon = c.icon}
            <button class="tab" aria-current={lite.category === c.id && !lite.stepsOpen ? "page" : undefined} data-testid="cat-{c.id}" onclick={() => selectCategory(c.id)}>
              <Icon size={20} />
              <span>{t(CATEGORY_LABELS[c.id])}</span>
            </button>
          {/each}
        </nav>
      {:else}
        <aside class="panel" aria-label={lite.stepsOpen ? t("Steps") : t(CATEGORY_LABELS[lite.category])}>
          <header class="panel__head">
            <div>
              <h2>{lite.stepsOpen ? t("Steps") : t(CATEGORY_LABELS[lite.category])}</h2>
              <p>{lite.stepsOpen ? t("Tap a step to go back to it.") : t(current.blurb)}</p>
            </div>
            {#if lite.stepsOpen}
              <button class="oa-icon-btn" aria-label={t("Close steps")} onclick={() => (lite.stepsOpen = false)}><X size={18} /></button>
            {/if}
          </header>
          <div class="panel__body">
            {#if lite.stepsOpen}
              <StepsPanel />
            {:else}
              {@render panel()}
            {/if}
          </div>
        </aside>
      {/if}
    </div>
  </div>
{/if}

{#snippet panel()}
  {#key lite.category}
    <div class="panel__content" data-testid="panel-{lite.category}">
      {#if lite.category === "auto"}
        <AutoPanel />
      {:else if lite.category === "adjust"}
        <AdjustPanel />
      {:else if lite.category === "crop"}
        <CropPanel />
      {:else if lite.category === "retouch"}
        <RetouchPanel />
      {:else if lite.category === "markup"}
        <MarkupPanel />
      {:else if lite.category === "effects"}
        <EffectsPanel />
      {:else}
        <EnhancePanel />
      {/if}
    </div>
  {/key}
{/snippet}

{#if lite.menuOpen && menuAnchor}
  <Menu items={menuItems} anchor={menuAnchor} autofocus testid="lite-menu" onclose={() => (lite.menuOpen = false)} />
{/if}
<ExportSheet />
<ToolSearch />

<style>
  .lite {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-page);
  }

  /* ---- Top bar ---- */
  .bar {
    flex: 0 0 auto;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: var(--space-3);
    height: var(--topbar-h);
    padding: 0 var(--space-3) 0 var(--space-4);
    border-bottom: var(--border-width) solid var(--border-hairline);
    background: var(--bg-page);
  }
  .bar__start,
  .bar__mid,
  .bar__end {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .bar__end {
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .brand {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border-hairline);
    margin: 0 var(--space-2);
    flex: 0 0 auto;
  }
  .file {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }
  .name {
    font: var(--type-ui);
    color: var(--text-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dims {
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .compare {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: var(--control-h-md);
    padding: 0 var(--space-3);
    margin-left: var(--space-1);
    border-radius: var(--radius-full);
    border: var(--border-width) solid var(--border-hairline);
    background: var(--surface-card);
    color: var(--text-body);
    font: var(--type-ui);
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    touch-action: none;
    transition: var(--transition-control);
  }
  .compare:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .compare.on {
    background: var(--text-strong);
    border-color: var(--text-strong);
    color: var(--bg-page);
  }
  .compare:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .search {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-h-md);
    padding: 0 var(--space-2) 0 var(--space-3);
    border-radius: var(--radius-md);
    border: var(--border-width) solid var(--border-hairline);
    background: var(--bg-subtle);
    color: var(--text-muted);
    font: var(--type-ui);
    font-weight: var(--weight-regular);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .search:hover {
    border-color: var(--border-strong);
    color: var(--text-strong);
  }
  .search kbd {
    font: var(--type-mono);
    font-size: var(--text-2xs);
    color: var(--text-faint);
    padding: 1px 5px;
    border-radius: var(--radius-xs);
    border: var(--border-width) solid var(--border-hairline);
    background: var(--surface-card);
    margin-left: var(--space-3);
  }

  /* ---- Body ---- */
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .rail {
    flex: 0 0 auto;
    width: 76px;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-2);
    border-right: var(--border-width) solid var(--border-hairline);
    background: var(--bg-page);
    overflow-y: auto;
  }
  .rail__btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: var(--space-2) 0;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .rail__icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 32px;
    border-radius: var(--radius-full);
    transition: var(--transition-control);
  }
  .rail__label {
    font: var(--type-caption);
    font-weight: var(--weight-medium);
  }
  .rail__btn:hover {
    color: var(--text-strong);
  }
  .rail__btn:hover .rail__icon {
    background: var(--surface-hover);
  }
  .rail__btn[aria-current="page"] {
    color: var(--text-strong);
  }
  .rail__btn[aria-current="page"] .rail__icon {
    background: var(--text-strong);
    color: var(--bg-page);
  }

  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--ops-pasteboard);
  }
  .canvas {
    position: absolute;
    inset: 0;
    bottom: var(--lite-prompt-room, 0px);
    transition: transform var(--duration-instant) linear;
  }
  .grid {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background-image: linear-gradient(to right, rgb(255 255 255 / 0.35) 1px, transparent 1px), linear-gradient(to bottom, rgb(255 255 255 / 0.35) 1px, transparent 1px);
    background-size: 12.5% 12.5%;
    mix-blend-mode: difference;
  }
  .badge {
    position: absolute;
    top: var(--space-4);
    left: 50%;
    transform: translateX(-50%);
    padding: 4px 12px;
    border-radius: var(--radius-full);
    background: var(--surface-inverse);
    color: var(--text-inverse);
    font: var(--type-ui);
    pointer-events: none;
    box-shadow: var(--shadow-md);
  }
  .lite:not(.phone) .stage {
    --lite-prompt-room: 132px;
  }
  .float {
    position: absolute;
    left: 50%;
    bottom: var(--space-4);
    transform: translateX(-50%);
    width: min(640px, calc(100% - var(--space-8)));
    z-index: 5;
  }

  .panel {
    flex: 0 0 auto;
    width: 340px;
    display: flex;
    flex-direction: column;
    border-left: var(--border-width) solid var(--border-hairline);
    background: var(--bg-page);
  }
  .panel__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-4) var(--space-3) var(--space-5);
  }
  .panel__head h2 {
    margin: 0;
    font: var(--type-h4);
    letter-spacing: var(--tracking-heading);
    color: var(--text-strong);
  }
  .panel__head p {
    margin: 2px 0 0;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .panel__body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-2) var(--space-5) var(--space-6);
    scrollbar-gutter: stable;
  }
  .panel__content {
    animation: enter var(--duration-base) var(--ease-out);
  }
  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }

  /* ---- Phone ---- */
  .phone .bar {
    display: flex;
    padding: 0 var(--space-2) 0 var(--space-3);
    padding-top: var(--safe-top);
    height: calc(var(--topbar-h) + var(--safe-top));
    gap: var(--space-1);
  }
  .phone .bar__start {
    flex: 0 0 auto;
  }
  .phone .bar__mid {
    flex: 1;
    justify-content: center;
  }
  .phone .bar__end {
    gap: var(--space-1);
    flex: 0 0 auto;
  }
  .phone .compare {
    width: var(--control-h-md);
    padding: 0;
    justify-content: center;
  }
  .phone .export {
    padding: 0 var(--space-3);
  }
  .phone .body {
    flex-direction: column;
  }
  .sheet {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    max-height: 48dvh;
    background: var(--bg-page);
    border-top: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xl) var(--radius-xl) 0 0;
    margin-top: calc(var(--radius-xl) * -1);
    position: relative;
    z-index: 2;
    box-shadow: 0 -8px 24px rgb(0 0 0 / 0.06);
  }
  .sheet__head {
    flex: 0 0 auto;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-2);
    min-height: 48px;
    padding: var(--space-1) var(--space-3) 0 var(--space-4);
    border: 0;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .sheet__title {
    font: var(--type-ui);
    font-size: var(--text-base);
    color: var(--text-strong);
  }
  .sheet__blurb {
    font: var(--type-caption);
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    color: var(--text-muted);
    transform: rotate(180deg);
    transition: transform var(--duration-base) var(--ease-standard);
  }
  .chev.down {
    transform: none;
  }
  .sheet__body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: var(--space-1) var(--space-4) var(--space-4);
  }
  .sheet__prompt {
    margin-bottom: var(--space-4);
  }
  .tabs {
    flex: 0 0 auto;
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    height: calc(var(--tabbar-h) + var(--safe-bottom));
    padding-bottom: var(--safe-bottom);
    border-top: var(--border-width) solid var(--border-hairline);
    background: var(--bg-page);
  }
  .tab {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-sans);
    transition: color var(--duration-fast) var(--ease-standard);
  }
  .tab span {
    white-space: nowrap;
    letter-spacing: -0.01em;
  }
  .tab[aria-current="page"] {
    color: var(--text-strong);
  }
  .tab[aria-current="page"] :global(svg) {
    stroke-width: 2.25;
  }
</style>
