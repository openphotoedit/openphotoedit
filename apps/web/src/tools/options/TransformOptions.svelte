<script lang="ts">
  // Free transform options: numeric X, Y, W, H, Scale and Angle fields
  // (applied live; ↑/↓ step 1, Shift ×10), the proportions link, flips,
  // quarter turns, Cancel and Apply.
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import FlipH from "@lucide/svelte/icons/flip-horizontal-2";
  import FlipV from "@lucide/svelte/icons/flip-vertical-2";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Link from "@lucide/svelte/icons/link-2";
  import Unlink from "@lucide/svelte/icons/link-2-off";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import NumberField from "../../ui/NumberField.svelte";
  import { toolSettings } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import { cancelTransform, commitTransform, setTransformField, transformBy } from "../transform";
  import { transformFields as f } from "../transform-state.svelte";
  import Row from "./Row.svelte";

  const off = $derived(!f.ready);
  const sizeOff = $derived(!f.ready || f.distorted);
  const set = (field: "x" | "y" | "w" | "h" | "scale" | "angle") => (v: number) => setTransformField(editor, field, v);
</script>

<Row testid="options-transform">
  <NumberField label="X" value={f.x} step={1} precision={0} unit="px" width={64} disabled={off} testid="transform-x" oninput={set("x")} />
  <NumberField label="Y" value={f.y} step={1} precision={0} unit="px" width={64} disabled={off} testid="transform-y" oninput={set("y")} />
  <NumberField label="W" value={f.w} min={1} step={1} precision={0} unit="px" width={64} disabled={sizeOff} testid="transform-w" oninput={set("w")} />
  <button
    class="oa-icon-btn oa-icon-btn--sm"
    class:on={toolSettings.transformKeepRatio}
    type="button"
    title={t("Keep proportions")}
    aria-label={t("Keep proportions")}
    aria-pressed={toolSettings.transformKeepRatio}
    data-testid="transform-link"
    onclick={() => (toolSettings.transformKeepRatio = !toolSettings.transformKeepRatio)}
  >
    {#if toolSettings.transformKeepRatio}<Link size={14} />{:else}<Unlink size={14} />{/if}
  </button>
  <NumberField label="H" value={f.h} min={1} step={1} precision={0} unit="px" width={64} disabled={sizeOff} testid="transform-h" oninput={set("h")} />
  <NumberField label={t("Scale")} value={f.scale} min={1} max={10000} step={1} precision={1} unit="%" width={60} disabled={sizeOff} testid="transform-scale" oninput={set("scale")} />
  <NumberField label="∠" ariaLabel={t("Angle")} value={f.angle} min={-180} max={180} step={1} precision={1} unit="°" width={56} disabled={sizeOff} testid="transform-angle" oninput={set("angle")} />
  <span class="ops-topt__sep"></span>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Flip horizontal")} aria-label={t("Flip horizontal")} onclick={() => transformBy(editor, "flip-h")}><FlipH size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Flip vertical")} aria-label={t("Flip vertical")} onclick={() => transformBy(editor, "flip-v")}><FlipV size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Rotate 90° counter-clockwise")} aria-label={t("Rotate 90° counter-clockwise")} onclick={() => transformBy(editor, "rotate-ccw")}><RotateCcw size={16} /></button>
  <button class="oa-icon-btn oa-icon-btn--sm" type="button" title={t("Rotate 90° clockwise")} aria-label={t("Rotate 90° clockwise")} onclick={() => transformBy(editor, "rotate-cw")}><RotateCw size={16} /></button>
  {#if f.layers > 1}<span class="ops-topt__label" data-testid="transform-layers">{t("{n} layers", { n: f.layers })}</span>{/if}
  <span class="ops-topt__sep"></span>
  <button class="oa-btn oa-btn--ghost" type="button" onclick={() => cancelTransform(editor)} data-testid="transform-cancel"><X size={16} />{t("Cancel")}</button>
  <button class="oa-btn oa-btn--primary" type="button" disabled={!toolState.transformActive} onclick={() => commitTransform(editor)} data-testid="transform-apply"><Check size={16} />{t("Apply")}</button>
</Row>

<style>
  .on {
    color: var(--text-strong);
    background: var(--surface-active);
  }
</style>
