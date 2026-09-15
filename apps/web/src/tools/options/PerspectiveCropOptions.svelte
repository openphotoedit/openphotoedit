<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { cancelPerspectiveCrop, commitPerspectiveCrop } from "../perspective-crop";
  import { toolState } from "../state.svelte";
  import Row from "./Row.svelte";
</script>

<Row testid="options-perspective-crop">
  <span class="ops-topt__label">{t("Drag a box, then move its corners onto the edges of the object.")}</span>
  <button class="oa-btn oa-btn--ghost" type="button" disabled={!toolState.perspectivePending} onclick={() => cancelPerspectiveCrop(editor)}><X size={16} />{t("Cancel")}</button>
  <button class="oa-btn oa-btn--primary" type="button" disabled={!toolState.perspectivePending} onclick={() => commitPerspectiveCrop(editor)}><Check size={16} />{t("Crop")}</button>
</Row>
