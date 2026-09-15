<script lang="ts">
  // Zoom field, document size and resolution, and the current tool's hint.
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import NumberField from "../ui/NumberField.svelte";
  import { pro } from "./state.svelte";
  import { toolMeta } from "./tools.svelte";

  const s = $derived(editor.summary);
  const mb = $derived(s ? (s.width * s.height * 4) / (1024 * 1024) : 0);
</script>

<footer class="status" data-testid="status-bar">
  {#if editor.hasDocument && s}
    <NumberField
      value={Math.round(editor.view.zoom * 1000) / 10}
      min={1}
      max={6400}
      step={1}
      precision={editor.view.zoom < 1 ? 1 : 0}
      unit="%"
      width={62}
      ariaLabel={t("Zoom")}
      testid="zoom-field"
      onchange={(v) => editor.zoomAt(v / 100)}
    />
    <span class="item mono">{s.width} × {s.height} px</span>
    <span class="item mono">{s.resolution} ppi</span>
    <span class="item mono">{mb < 1 ? `${Math.round(mb * 1024)}K` : `${mb.toFixed(1)}M`}</span>
    {#if s.selection}
      <span class="item mono">{t("Selection")} {s.selection.bounds.w} × {s.selection.bounds.h}</span>
    {/if}
    {#if pro.cursor}
      <span class="item mono">{Math.floor(pro.cursor.x)}, {Math.floor(pro.cursor.y)}</span>
    {/if}
  {/if}
  <span class="hint">{toolMeta(editor.tool).hint}</span>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 26px;
    flex: 0 0 auto;
    padding: 0 var(--space-2);
    background: var(--bg-page);
    border-top: var(--border-width) solid var(--border-hairline);
    font: var(--type-caption);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    --ops-control-h: 20px;
  }
  .item {
    color: var(--text-muted);
  }
  .mono {
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
