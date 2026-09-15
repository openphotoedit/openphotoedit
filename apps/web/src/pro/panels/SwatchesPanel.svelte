<script lang="ts">
  // Swatches: click sets the foreground colour, Alt-click the background.
  // Add the current foreground; right-click a swatch to remove it.
  import Plus from "@lucide/svelte/icons/plus";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import IconButton from "../../ui/IconButton.svelte";
  import { fromHex, toHex } from "../../ui/color";
  import { tooltip } from "../../ui/tooltip";
  import { pro } from "../state.svelte";
</script>

<div class="panel" data-testid="swatches-panel">
  <div class="grid">
    {#each pro.layout.swatches as hex, i (hex + i)}
      <button
        type="button"
        class="sw"
        style:background="#{hex}"
        aria-label="#{hex}"
        use:tooltip={`#${hex}`}
        onclick={(e) => {
          const c = fromHex(hex);
          if (!c) return;
          if (e.altKey) editor.secondary = c;
          else editor.primary = c;
        }}
        oncontextmenu={(e) => {
          e.preventDefault();
          pro.layout.swatches = pro.layout.swatches.filter((_, j) => j !== i);
          pro.save();
        }}
      ></button>
    {/each}
    <IconButton
      size="xs"
      label={t("Add the foreground colour")}
      onclick={() => {
        pro.layout.swatches = [...pro.layout.swatches, toHex(editor.primary)];
        pro.save();
      }}><Plus size={12} /></IconButton
    >
  </div>
  <p class="ops-note">{t("Alt-click sets the background colour. Right-click removes a swatch.")}</p>
</div>

<style>
  .panel {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, 20px);
    gap: 3px;
  }
  .sw {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-hairline);
    border-radius: 2px;
    cursor: default;
  }
  .sw:hover {
    border-color: var(--text-strong);
  }
</style>
