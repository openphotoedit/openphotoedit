<script lang="ts">
  // Adjustments: one button per adjustment layer kind.
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { tooltip } from "../../ui/tooltip";
  import { ADJUSTMENT_KINDS } from "../adjustments";
  import { newAdjustmentLayer } from "../layer-ops";
</script>

<div class="panel" data-testid="adjustments-panel">
  <p class="lead">{t("Add an adjustment")}</p>
  <div class="grid">
    {#each ADJUSTMENT_KINDS as k, i (k?.kind ?? `sep${i}`)}
      {#if k}
        {@const Icon = k.icon}
        <button type="button" class="adj" aria-label={k.label} data-testid="adjustments-{k.kind}" disabled={!editor.hasDocument} use:tooltip={k.label} onclick={() => newAdjustmentLayer(k.kind)}>
          <Icon size={16} />
        </button>
      {/if}
    {/each}
  </div>
</div>

<style>
  .panel {
    padding: 10px;
  }
  .lead {
    margin: 0 0 8px;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, 30px);
    gap: 4px;
  }
  .adj {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    cursor: default;
  }
  .adj:hover:not(:disabled) {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .adj:disabled {
    opacity: 0.4;
  }
</style>
