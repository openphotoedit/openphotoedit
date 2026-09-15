<script lang="ts">
  // Spot healing, remove, patch and red eye.
  import { t } from "../../lib/i18n";
  import { setSizeFor, sizeFor, toolSettings, type Quality } from "../settings.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";

  let { tool }: { tool: string } = $props();

  const size = {
    get value() {
      return sizeFor(tool);
    },
    set value(v: number) {
      setSizeFor(tool, v);
    },
  };
  const qualities: { value: Quality; label: string }[] = [
    { value: "fast", label: t("Fast") },
    { value: "best", label: t("Best") },
  ];
</script>

<Row testid="options-{tool}">
  {#if tool === "spot-heal" || tool === "remove"}
    <Range label={t("Size")} bind:value={size.value} min={1} max={1000} unit="px" />
  {/if}
  {#if tool === "remove"}
    <Segmented label={t("Quality")} bind:value={toolSettings.removeQuality} options={qualities} />
    <span class="ops-topt__label">{t("Paint over what to remove, then let go.")}</span>
  {:else if tool === "patch"}
    <Range label={t("Blend")} bind:value={toolSettings.patchBlend} min={0} max={1} percent unit="%" />
    <span class="ops-topt__label">{t("Draw around the area, then drag it onto clean texture.")}</span>
  {:else if tool === "red-eye"}
    <Range label={t("Pupil size")} bind:value={toolSettings.redEyePupil} min={0.01} max={1} percent unit="%" />
    <Range label={t("Darken")} bind:value={toolSettings.redEyeDarken} min={0.01} max={1} percent unit="%" />
  {/if}
</Row>
