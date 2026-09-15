<script lang="ts">
  // Marquee, lasso, object selection, magic wand and quick selection.
  import Square from "@lucide/svelte/icons/square";
  import SquarePlus from "@lucide/svelte/icons/square-plus";
  import SquareMinus from "@lucide/svelte/icons/square-minus";
  import SquareSplit from "@lucide/svelte/icons/square-split-horizontal";
  import { t } from "../../lib/i18n";
  import { toolSettings, type SelectMode } from "../settings.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Range from "./Range.svelte";
  import Toggle from "./Toggle.svelte";

  let { tool }: { tool: string } = $props();

  const modes: { value: SelectMode; label: string; icon: typeof Square }[] = [
    { value: "replace", label: t("New selection"), icon: Square },
    { value: "add", label: t("Add to selection"), icon: SquarePlus },
    { value: "subtract", label: t("Subtract from selection"), icon: SquareMinus },
    { value: "intersect", label: t("Intersect with selection"), icon: SquareSplit },
  ];
  const quickModes = $derived(modes.filter((m) => m.value === "add" || m.value === "subtract"));
</script>

<Row testid="options-{tool}">
  {#if tool === "quick-select"}
    <Segmented label={t("Mode")} bind:value={toolSettings.selectMode} options={quickModes} />
    <Range label={t("Size")} bind:value={toolSettings.quickSelectSize} min={1} max={500} unit="px" />
  {:else}
    <Segmented bind:value={toolSettings.selectMode} options={tool === "object-select" ? modes.slice(0, 3) : modes} testid="select-mode" />
  {/if}
  {#if tool === "marquee-rect" || tool === "marquee-ellipse" || tool === "lasso" || tool === "polygon-lasso"}
    <Range label={t("Feather")} bind:value={toolSettings.feather} min={0} max={250} unit="px" />
  {/if}
  {#if tool === "marquee-ellipse" || tool === "lasso" || tool === "polygon-lasso" || tool === "magic-wand"}
    <Toggle label={t("Anti-alias")} bind:checked={toolSettings.antiAlias} />
  {/if}
  {#if tool === "magic-wand"}
    <Range label={t("Tolerance")} bind:value={toolSettings.wandTolerance} min={0} max={255} />
    <Toggle label={t("Contiguous")} bind:checked={toolSettings.wandContiguous} />
    <Toggle label={t("Sample all layers")} bind:checked={toolSettings.wandSampleAll} />
  {/if}
</Row>
