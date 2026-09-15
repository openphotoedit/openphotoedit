<script lang="ts">
  // Gradient and paint bucket.
  import { t } from "../../lib/i18n";
  import { toolSettings, type GradientType } from "../settings.svelte";
  import Choice from "./Choice.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Toggle from "./Toggle.svelte";

  let { tool }: { tool: string } = $props();

  const types: { value: GradientType; label: string }[] = [
    { value: "linear", label: t("Linear") },
    { value: "radial", label: t("Radial") },
    { value: "angle", label: t("Angle") },
    { value: "reflected", label: t("Reflected") },
    { value: "diamond", label: t("Diamond") },
  ];
  const presets = [
    { value: "fg-bg" as const, label: t("Foreground to background") },
    { value: "fg-transparent" as const, label: t("Foreground to transparent") },
    { value: "black-white" as const, label: t("Black to white") },
  ];
</script>

<Row testid="options-{tool}">
  {#if tool === "gradient"}
    <Choice label={t("Gradient")} bind:value={toolSettings.gradientPreset} options={presets} />
    <Segmented bind:value={toolSettings.gradientType} options={types} />
    <Toggle label={t("Reverse")} bind:checked={toolSettings.gradientReverse} />
    <Range label={t("Opacity")} bind:value={toolSettings.gradientOpacity} min={0.01} max={1} percent unit="%" />
  {:else}
    <Range label={t("Tolerance")} bind:value={toolSettings.bucketTolerance} min={0} max={255} />
    <Range label={t("Opacity")} bind:value={toolSettings.bucketOpacity} min={0.01} max={1} percent unit="%" />
    <Toggle label={t("Anti-alias")} bind:checked={toolSettings.bucketAntiAlias} />
    <Toggle label={t("Contiguous")} bind:checked={toolSettings.bucketContiguous} />
    <Toggle label={t("All layers")} bind:checked={toolSettings.bucketSampleAll} />
  {/if}
</Row>
