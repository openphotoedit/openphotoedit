<script lang="ts">
  // Shapes and markup: rectangle, ellipse, line, arrow, pen, highlighter,
  // redact and pixelate. Changes restyle the selected shape too.
  import Trash from "@lucide/svelte/icons/trash-2";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { run } from "../common";
  import { toolSettings } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import Color from "./Color.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";

  let { tool }: { tool: string } = $props();

  const boxy = $derived(tool === "shape-rect" || tool === "shape-ellipse");
  const lined = $derived(tool === "shape-line" || tool === "arrow");

  const highlightOpacity = {
    get value() {
      return (toolSettings.highlighterColor.a ?? 255) / 255;
    },
    set value(v: number) {
      toolSettings.highlighterColor = { ...toolSettings.highlighterColor, a: Math.round(Math.max(0.05, Math.min(1, v)) * 255) };
    },
  };

  function deleteSelected() {
    const id = toolState.annotationSelected;
    if (id == null) return;
    toolState.annotationSelected = null;
    void run(editor, { op: "layer.delete", ids: [id] });
  }
</script>

<Row testid="options-{tool}">
  {#if boxy}
    <Toggle label={t("Stroke")} bind:checked={toolSettings.shapeStrokeOn} />
    <Color label={t("Stroke colour")} bind:value={toolSettings.shapeStroke} fallback={editor.primary} testid="shape-stroke" />
    <Toggle label={t("Fill")} bind:checked={toolSettings.shapeFillOn} />
    <Color label={t("Fill colour")} bind:value={toolSettings.shapeFill} fallback={editor.secondary} />
    <Range label={t("Width")} bind:value={toolSettings.shapeWidth} min={1} max={100} unit="px" />
    {#if tool === "shape-rect"}
      <Range label={t("Corner radius")} bind:value={toolSettings.shapeRadius} min={0} max={500} unit="px" />
    {/if}
    <Toggle label={t("Dashed")} bind:checked={toolSettings.shapeDashed} />
  {:else if lined}
    <Color label={t("Colour")} bind:value={toolSettings.shapeStroke} fallback={editor.primary} testid="shape-stroke" />
    <Range label={t("Width")} bind:value={toolSettings.shapeWidth} min={1} max={100} unit="px" />
    {#if tool === "arrow"}
      <Toggle label={t("Head at start")} bind:checked={toolSettings.arrowStart} />
      <Toggle label={t("Head at end")} bind:checked={toolSettings.arrowEnd} />
    {/if}
    <Toggle label={t("Dashed")} bind:checked={toolSettings.shapeDashed} />
  {:else if tool === "pen"}
    <Color label={t("Colour")} bind:value={toolSettings.shapeStroke} fallback={editor.primary} />
    <Range label={t("Width")} bind:value={toolSettings.penWidth} min={1} max={100} unit="px" />
    <Range label={t("Smoothing")} bind:value={toolSettings.penSmoothing} min={0} max={1} percent unit="%" />
  {:else if tool === "highlighter"}
    <Color label={t("Colour")} bind:value={toolSettings.highlighterColor} />
    <Range label={t("Width")} bind:value={toolSettings.highlighterWidth} min={4} max={200} unit="px" />
    <Range label={t("Opacity")} bind:value={highlightOpacity.value} min={0.05} max={1} percent unit="%" />
  {:else if tool === "redact"}
    <Color label={t("Colour")} bind:value={toolSettings.redactColor} />
  {:else if tool === "pixelate"}
    <Range label={t("Cell size")} bind:value={toolSettings.pixelateCell} min={2} max={128} unit="px" />
    <span class="ops-topt__label">{t("Drag over what to hide. The detail underneath is gone for good.")}</span>
  {/if}
  {#if toolState.annotationSelected != null && tool !== "pixelate"}
    <span class="ops-topt__sep"></span>
    <button class="oa-btn oa-btn--ghost" type="button" onclick={deleteSelected}><Trash size={16} />{t("Delete")}</button>
  {/if}
</Row>
