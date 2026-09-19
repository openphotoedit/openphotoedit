<script lang="ts">
  // Liquify: mode, brush size, pressure and density.
  import { t } from "../../lib/i18n";
  import { LIQUIFY_MODES, liquifySettings, setLiquifySize } from "../liquify.svelte";
  import Choice from "./Choice.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";

  let { tool }: { tool: string } = $props();

  const modes = LIQUIFY_MODES.map((m) => ({ value: m.value, label: t(m.label) }));
  const size = {
    get value() {
      return liquifySettings.size;
    },
    set value(v: number) {
      setLiquifySize(v);
    },
  };
</script>

<Row testid="options-{tool}">
  <Choice label={t("Mode")} bind:value={liquifySettings.mode} options={modes} testid="liquify-mode" />
  <Range label={t("Size")} bind:value={size.value} min={1} max={2000} unit="px" testid="liquify-size" />
  <Range label={t("Pressure")} bind:value={liquifySettings.pressure} min={0.01} max={1} percent unit="%" testid="liquify-pressure" />
  <Range label={t("Density")} bind:value={liquifySettings.density} min={0} max={1} percent unit="%" />
</Row>
