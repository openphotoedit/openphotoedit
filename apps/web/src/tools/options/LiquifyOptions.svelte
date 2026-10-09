<script lang="ts">
  // Liquify: mode, brush size, pressure and density, then Cancel and OK,
  // which end the session as Escape and Enter do.
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { closeLiquify, LIQUIFY_MODES, liquifySession, liquifySettings, setLiquifySize } from "../liquify.svelte";
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
  <span class="ops-topt__sep"></span>
  <button class="oa-btn oa-btn--ghost" type="button" disabled={liquifySession.closing} onclick={() => closeLiquify(editor, false)} data-testid="liquify-cancel"><X size={16} />{t("Cancel")}</button>
  <button class="oa-btn oa-btn--primary" type="button" disabled={liquifySession.closing} onclick={() => closeLiquify(editor, true)} data-testid="liquify-ok"><Check size={16} />{t("OK")}</button>
</Row>
