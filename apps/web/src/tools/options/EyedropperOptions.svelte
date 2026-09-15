<script lang="ts">
  import { t } from "../../lib/i18n";
  import { toolSettings } from "../settings.svelte";
  import Choice from "./Choice.svelte";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";

  const sizes: { value: 1 | 3 | 5; label: string }[] = [
    { value: 1, label: t("Point sample") },
    { value: 3, label: t("3 by 3 average") },
    { value: 5, label: t("5 by 5 average") },
  ];
</script>

<Row testid="options-eyedropper">
  <Choice label={t("Sample size")} bind:value={toolSettings.sampleSize} options={sizes} />
  <Toggle label={t("Sample all layers")} bind:checked={toolSettings.sampleMerged} />
  <span class="ops-topt__label">{t("Alt-click sets the background colour.")}</span>
</Row>
