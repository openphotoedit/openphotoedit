<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import FlipH from "@lucide/svelte/icons/flip-horizontal-2";
  import FlipV from "@lucide/svelte/icons/flip-vertical-2";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { toolSettings } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import { cancelTransform, commitTransform, transformBy } from "../transform";
  import Row from "./Row.svelte";
  import Toggle from "./Toggle.svelte";
</script>

<Row testid="options-transform">
  <span class="ops-topt__label" data-testid="transform-info">W {toolState.transformInfo.w} px · H {toolState.transformInfo.h} px · {toolState.transformInfo.angle}°</span>
  <Toggle label={t("Keep proportions")} bind:checked={toolSettings.transformKeepRatio} />
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Flip horizontal")} aria-label={t("Flip horizontal")} onclick={() => transformBy(editor, "flip-h")}><FlipH size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Flip vertical")} aria-label={t("Flip vertical")} onclick={() => transformBy(editor, "flip-v")}><FlipV size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Rotate 90° counter-clockwise")} aria-label={t("Rotate 90° counter-clockwise")} onclick={() => transformBy(editor, "rotate-ccw")}><RotateCcw size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Rotate 90° clockwise")} aria-label={t("Rotate 90° clockwise")} onclick={() => transformBy(editor, "rotate-cw")}><RotateCw size={16} /></button>
  <span class="ops-topt__label">{t("Shift unlocks proportions, Alt scales from the centre, Cmd-drag a corner distorts.")}</span>
  <span class="ops-topt__sep"></span>
  <button class="oa-btn oa-btn--ghost" type="button" onclick={() => cancelTransform(editor)} data-testid="transform-cancel"><X size={16} />{t("Cancel")}</button>
  <button class="oa-btn oa-btn--primary" type="button" disabled={!toolState.transformActive} onclick={() => commitTransform(editor)} data-testid="transform-apply"><Check size={16} />{t("Apply")}</button>
</Row>
