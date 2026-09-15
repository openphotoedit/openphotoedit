<script lang="ts">
  // Brush, pencil, eraser, clone, healing brush and the toning brushes.
  import { BLEND_LABELS, BLEND_MODES, type BlendMode } from "../../engine/types";
  import { t } from "../../lib/i18n";
  import { setSizeFor, sizeFor, toolSettings, type ToneRange } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import Choice from "./Choice.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";

  let { tool }: { tool: string } = $props();

  const blends = BLEND_MODES.filter((b): b is BlendMode => !!b).map((b) => ({ value: b, label: t(BLEND_LABELS[b]) }));
  const ranges: { value: ToneRange; label: string }[] = [
    { value: "shadows", label: t("Shadows") },
    { value: "midtones", label: t("Midtones") },
    { value: "highlights", label: t("Highlights") },
  ];

  const toning = $derived(["dodge", "burn", "sponge", "blur-brush", "sharpen-brush", "smudge"].includes(tool));
  const size = {
    get value() {
      return sizeFor(tool);
    },
    set value(v: number) {
      setSizeFor(tool, v);
    },
  };
</script>

<Row testid="options-{tool}">
  <Range label={t("Size")} bind:value={size.value} min={1} max={1000} unit="px" testid="brush-size" />
  {#if tool === "eraser"}
    <Choice
      label={t("Mode")}
      bind:value={toolSettings.eraserMode}
      options={[
        { value: "brush", label: t("Brush") },
        { value: "pencil", label: t("Pencil") },
      ]}
    />
    {#if toolSettings.eraserMode === "brush"}
      <Range label={t("Hardness")} bind:value={toolSettings.eraserHardness} min={0} max={1} percent unit="%" />
    {/if}
    <Range label={t("Opacity")} bind:value={toolSettings.eraserOpacity} min={0.01} max={1} percent unit="%" />
    <Range label={t("Flow")} bind:value={toolSettings.eraserFlow} min={0.01} max={1} percent unit="%" />
  {:else}
    {#if tool !== "pencil"}
      <Range label={t("Hardness")} bind:value={toolSettings.brushHardness} min={0} max={1} percent unit="%" />
    {/if}
    {#if tool === "brush" || tool === "pencil"}
      <Choice label={t("Mode")} bind:value={toolSettings.brushBlend} options={blends} />
    {/if}
    {#if !toning}
      <Range label={t("Opacity")} bind:value={toolSettings.brushOpacity} min={0.01} max={1} percent unit="%" testid="brush-opacity" />
      <Range label={t("Flow")} bind:value={toolSettings.brushFlow} min={0.01} max={1} percent unit="%" />
    {/if}
  {/if}
  {#if tool === "dodge" || tool === "burn"}
    <Choice label={t("Range")} bind:value={toolSettings.toneRange} options={ranges} />
    <Range label={t("Exposure")} bind:value={toolSettings.toneExposure} min={0.01} max={1} percent unit="%" />
  {:else if tool === "sponge"}
    <Choice
      label={t("Mode")}
      bind:value={toolSettings.spongeMode}
      options={[
        { value: "desaturate", label: t("Desaturate") },
        { value: "saturate", label: t("Saturate") },
      ]}
    />
    <Range label={t("Flow")} bind:value={toolSettings.spongeFlow} min={0.01} max={1} percent unit="%" />
  {:else if tool === "blur-brush" || tool === "sharpen-brush" || tool === "smudge"}
    <Range label={t("Strength")} bind:value={toolSettings.strength} min={0.01} max={1} percent unit="%" />
  {/if}
  {#if tool === "clone" || tool === "heal"}
    <Toggle label={t("Aligned")} bind:checked={toolSettings.cloneAligned} />
    <Toggle label={t("Sample all layers")} bind:checked={toolSettings.cloneSampleAll} />
    {#if !toolState.sourceSet}
      <span class="ops-topt__label">{t("Alt-click to set the source.")}</span>
    {/if}
  {/if}
  {#if tool !== "pencil"}
    <Toggle label={t("Pen pressure: size")} bind:checked={toolSettings.pressureSize} />
  {/if}
</Row>
