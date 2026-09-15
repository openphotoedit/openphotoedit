<script lang="ts">
  import { t } from "../../lib/i18n";
  import { toolSettings } from "../settings.svelte";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";
</script>

<Row testid="options-move">
  <Toggle label={t("Auto-select layer")} bind:checked={toolSettings.autoSelect} testid="move-auto-select" />
  <span class="ops-topt__label">{t("Arrow keys nudge 1 px, Shift 10 px. Alt-drag duplicates.")}</span>
</Row>
