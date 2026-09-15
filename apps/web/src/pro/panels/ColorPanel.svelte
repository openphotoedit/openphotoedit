<script lang="ts">
  // Foreground and background colours with an HSB/RGB picker.
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import { css, fromHex, toHex } from "../../ui/color";
  import type { Rgba8 } from "../../engine/types";
  import { swapColors } from "../colors";

  let which = $state<"primary" | "secondary">("primary");
  const current = $derived(which === "primary" ? editor.primary : editor.secondary);
  function set(c: Rgba8) {
    if (which === "primary") editor.primary = c;
    else editor.secondary = c;
  }
</script>

<div class="panel" data-testid="color-panel">
  <div class="head">
    <div class="pair">
      <button type="button" class="chip bg" class:editing={which === "secondary"} style:background={css(editor.secondary)} aria-label={t("Edit background colour")} aria-pressed={which === "secondary"} onclick={() => (which = "secondary")}></button>
      <button type="button" class="chip fg" class:editing={which === "primary"} style:background={css(editor.primary)} aria-label={t("Edit foreground colour")} aria-pressed={which === "primary"} onclick={() => (which = "primary")}></button>
    </div>
    <IconButton size="xs" label={t("Switch colours")} shortcut="X" onclick={swapColors}><ArrowLeftRight size={11} /></IconButton>
    <span class="which">{which === "primary" ? t("Foreground") : t("Background")}</span>
  </div>
  <ColorPicker value={current} height={84} showFields={false} oninput={set} />
  <div class="fields">
    <NumberField label="R" value={current.r} min={0} max={255} width={40} onchange={(v) => set({ ...current, r: v })} />
    <NumberField label="G" value={current.g} min={0} max={255} width={40} onchange={(v) => set({ ...current, g: v })} />
    <NumberField label="B" value={current.b} min={0} max={255} width={40} onchange={(v) => set({ ...current, b: v })} />
    <label class="hex">
      <span class="ops-label">#</span>
      <input
        class="ops-field"
        spellcheck="false"
        aria-label={t("Hex colour")}
        value={toHex(current)}
        onchange={(e) => {
          const c = fromHex(e.currentTarget.value);
          if (c) set(c);
        }}
      />
    </label>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pair {
    position: relative;
    width: 34px;
    height: 28px;
  }
  .chip {
    position: absolute;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    cursor: default;
  }
  .chip.editing {
    box-shadow: 0 0 0 1px var(--bg-subtle), 0 0 0 2px var(--text-strong);
  }
  .fg {
    left: 0;
    top: 0;
    z-index: 1;
  }
  .bg {
    right: 0;
    bottom: 0;
  }
  .fields {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hex {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .hex input {
    width: 100%;
    font-family: var(--font-mono);
  }
  .which {
    font: var(--type-caption);
    color: var(--text-muted);
  }
</style>
