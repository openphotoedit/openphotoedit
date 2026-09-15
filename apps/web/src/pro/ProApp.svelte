<script lang="ts">
  // The Pro workspace: menu bar, options bar, toolbar, document tabs, canvas
  // with rulers, status bar and the panel dock. Photoshop's shortcuts are
  // handled here; the canvas handles Space and the active tool's own keys.
  import "./pro.css";
  import { onMount } from "svelte";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import BrandMark from "../ui/BrandMark.svelte";
  import MenuBar from "../ui/MenuBar.svelte";
  import { isTyping, modHeld } from "../ui/platform";
  import { paintTarget } from "../ui/paint-target.svelte";
  import { tooltip } from "../ui/tooltip";
  import { ACTIONS, MENUS, handleShortcut, loadFileBridge } from "./actions.svelte";
  import { resetColors, swapColors } from "./colors";
  import DialogHost from "./dialogs/DialogHost.svelte";
  import DocTabs from "./DocTabs.svelte";
  import Dock from "./Dock.svelte";
  import OptionsBar from "./OptionsBar.svelte";
  import Stage from "./Stage.svelte";
  import StatusBar from "./StatusBar.svelte";
  import Toolbar from "./Toolbar.svelte";
  import { pro } from "./state.svelte";
  import { loadToolModules, selectTool, toolForLetter } from "./tools.svelte";

  onMount(() => {
    void loadToolModules();
    void loadFileBridge();
    // Shortcuts run in the capture phase so a focused panel control does not
    // swallow Cmd+Z; typing in a field is left alone.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  // Navigation keys pressed in a panel, the menu bar or the toolbar belong to
  // that control: they must not also reach the canvas tool (an arrow key in
  // the Layers list would otherwise nudge the layer).
  const CONTROL_KEYS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Delete", "Backspace", "Enter", "Home", "End", "PageUp", "PageDown", " "]);
  function containKeys(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (!CONTROL_KEYS.has(e.key) || el === document.body || el.closest?.("[data-testid=canvas]")) return;
    e.stopPropagation();
  }

  function onKey(e: KeyboardEvent) {
    if (pro.dialog || document.querySelector(".ops-dialog-scrim, .ops-menu")) return;
    if (isTyping(e.target)) return;
    if (modHeld(e) || e.altKey || /^F\d+$/.test(e.key)) {
      if (handleShortcut(e)) e.stopPropagation();
      return;
    }
    // Plain keys: tools and colours.
    const target = e.target as HTMLElement;
    if (target.closest?.('[role="tree"], [role="slider"], [role="listbox"], .ops-curve')) {
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Delete", "Backspace", "Enter", "F2"].includes(e.key)) return;
    }
    if (e.key === "Tab" && (target === document.body || target.closest?.("[data-testid=canvas]"))) {
      e.preventDefault();
      void ACTIONS["window.dock"].run();
      return;
    }
    if ((e.key === "Delete" || e.key === "Backspace") && editor.summary?.selection) {
      e.preventDefault();
      void ACTIONS["edit.clear"].run();
      return;
    }
    if (e.repeat || e.metaKey || e.ctrlKey) return;
    const k = e.key.toLowerCase();
    if (k === "x" && !e.shiftKey) {
      swapColors();
      return;
    }
    if (k === "d" && !e.shiftKey) {
      resetColors();
      return;
    }
    if (k === "q" && !e.shiftKey && editor.hasDocument) {
      paintTarget.quickMask = !paintTarget.quickMask;
      return;
    }
    if (/^[a-z]$/.test(k)) {
      const id = toolForLetter(k, e.shiftKey);
      if (id) {
        selectTool(id);
        e.preventDefault();
      }
    }
  }
</script>

<div class="ops-pro oa-dense" data-testid="pro-app" role="presentation" onkeydown={containKeys}>
  <header class="menubar">
    <span class="brand"><BrandMark size={13} /></span>
    <MenuBar menus={MENUS} label={t("Application menu")} onopenchange={(o) => (pro.menuOpen = o)} />
    <span class="spacer"></span>
    <span class="local" use:tooltip={t("Nothing you open leaves this device.")}>{t("On this device")}</span>
    <button type="button" class="simple" data-testid="switch-simple" onclick={() => editor.setProfile("lite")}>{t("Switch to Simple")}</button>
  </header>
  <OptionsBar />
  <div class="work">
    <Toolbar />
    <main class="center">
      <DocTabs />
      <Stage />
      <StatusBar />
    </main>
    <Dock />
  </div>
</div>
<DialogHost />

<style>
  .ops-pro {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-page);
    color: var(--text-body);
    font: var(--weight-regular) var(--text-xs) / 1.3 var(--font-sans);
    /* A mid-dark pasteboard so the document edge reads, as Photoshop's does. */
    --ops-pasteboard: var(--surface-raised);
  }
  .menubar {
    display: flex;
    align-items: stretch;
    height: 30px;
    flex: 0 0 auto;
    padding: 0 6px 0 12px;
    background: var(--bg-page);
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .brand {
    display: flex;
    align-items: center;
    margin-right: 10px;
  }
  .spacer {
    flex: 1;
  }
  .local {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-right: 10px;
    font: var(--type-caption);
    color: var(--text-faint);
  }
  .local::before {
    content: "";
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
  .simple {
    align-self: center;
    height: 22px;
    padding: 0 10px;
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-xs);
    background: transparent;
    font: var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-body);
    cursor: default;
  }
  .simple:hover {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .work {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
</style>
