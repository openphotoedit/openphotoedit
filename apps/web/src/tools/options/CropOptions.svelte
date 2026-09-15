<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import Ruler from "@lucide/svelte/icons/ruler";
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { applyCropRatio, commitCrop, resetCrop, swapCropOrientation } from "../crop";
  import { toolSettings, type CropOverlay, type CropRatio } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import Choice from "./Choice.svelte";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";

  const ratios: { value: CropRatio; label: string }[] = [
    { value: "free", label: t("Free") },
    { value: "original", label: t("Original ratio") },
    { value: "1:1", label: "1 : 1" },
    { value: "4:5", label: "4 : 5" },
    { value: "3:2", label: "3 : 2" },
    { value: "16:9", label: "16 : 9" },
    { value: "9:16", label: "9 : 16" },
    { value: "custom", label: t("Custom") },
  ];
  const overlays: { value: CropOverlay; label: string }[] = [
    { value: "thirds", label: t("Rule of thirds") },
    { value: "grid", label: t("Grid") },
    { value: "none", label: t("None") },
  ];

  let lastRatio = toolSettings.cropRatio;
  $effect(() => {
    const r = toolSettings.cropRatio;
    void toolSettings.cropCustomW;
    void toolSettings.cropCustomH;
    if (r !== lastRatio || r === "custom") {
      lastRatio = r;
      if (r !== "free") applyCropRatio(editor);
    }
  });

  function num(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    return Number.isFinite(v) && v >= 0 ? Math.round(v) : 0;
  }
</script>

<Row testid="options-crop">
  <Choice label={t("Ratio")} bind:value={toolSettings.cropRatio} options={ratios} testid="crop-ratio" />
  {#if toolSettings.cropRatio === "custom"}
    <span class="ops-topt__field">
      <input class="oa-input ops-topt__num" type="number" min="1" aria-label={t("Ratio width")} value={toolSettings.cropCustomW} onchange={(e) => (toolSettings.cropCustomW = Math.max(1, num(e)))} />
      <span class="ops-topt__label">:</span>
      <input class="oa-input ops-topt__num" type="number" min="1" aria-label={t("Ratio height")} value={toolSettings.cropCustomH} onchange={(e) => (toolSettings.cropCustomH = Math.max(1, num(e)))} />
    </span>
  {/if}
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Swap width and height (X)")} aria-label={t("Swap width and height")} onclick={() => swapCropOrientation(editor)}>
    <ArrowLeftRight size={16} />
  </button>
  <span class="ops-topt__field">
    <span class="ops-topt__label">{t("Size")}</span>
    <input class="oa-input ops-topt__num" type="number" min="0" placeholder="W" aria-label={t("Output width in pixels")} value={toolSettings.cropOutW || ""} onchange={(e) => (toolSettings.cropOutW = num(e))} />
    <span class="ops-topt__label">×</span>
    <input class="oa-input ops-topt__num" type="number" min="0" placeholder="H" aria-label={t("Output height in pixels")} value={toolSettings.cropOutH || ""} onchange={(e) => (toolSettings.cropOutH = num(e))} />
    <span class="ops-topt__label">px</span>
  </span>
  <span class="ops-topt__sep"></span>
  <button
    class="oa-btn"
    class:oa-btn--secondary={toolState.cropStraighten}
    class:oa-btn--ghost={!toolState.cropStraighten}
    type="button"
    aria-pressed={toolState.cropStraighten}
    title={t("Drag a line along the horizon to straighten")}
    onclick={() => (toolState.cropStraighten = !toolState.cropStraighten)}
  >
    <Ruler size={16} />{t("Straighten")}
  </button>
  <Choice label={t("Overlay")} bind:value={toolSettings.cropOverlay} options={overlays} />
  <Toggle label={t("Delete cropped pixels")} bind:checked={toolSettings.cropDeleteCropped} />
  {#if toolState.cropPending}
    <span class="ops-topt__label" data-testid="crop-info">{toolState.cropInfo.w} × {toolState.cropInfo.h}{toolState.cropInfo.angle ? ` · ${toolState.cropInfo.angle}°` : ""}</span>
  {/if}
  <span class="ops-topt__sep"></span>
  <button class="oa-btn oa-btn--ghost" type="button" onclick={() => resetCrop(editor)} data-testid="crop-cancel"><X size={16} />{t("Cancel")}</button>
  <button class="oa-btn oa-btn--primary" type="button" onclick={() => commitCrop(editor)} data-testid="crop-apply"><Check size={16} />{t("Crop")}</button>
</Row>
