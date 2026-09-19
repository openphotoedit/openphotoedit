<script lang="ts">
  // Image › Adjustments: the same editors as adjustment layers, applied to
  // the active layer's pixels, with a live preview.
  import { onDestroy, onMount } from "svelte";
  import type { Adjustment } from "../../engine/types";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Dialog from "../../ui/Dialog.svelte";
  import type { HistogramData } from "../../ui/histogram";
  import AdjustmentEditor from "../adjust/AdjustmentEditor.svelte";
  import { sampleComposite } from "../adjust/canvas-pick";
  import { defaultAdjustment, kindInfo } from "../adjustments";
  import { PreviewSession, histogram, isUnknownOp } from "../engine.svelte";
  import { applyAdjustment } from "../layer-ops";
  import { pro } from "../state.svelte";

  let { adjKind, initial }: { adjKind: string; initial?: Adjustment } = $props();

  // `initial` can arrive as reactive state; snapshot it before cloning.
  const init = () => structuredClone($state.snapshot(initial) ?? defaultAdjustment(adjKind)) as Adjustment;
  let value = $state<Adjustment>(init());
  let preview = $state(true);
  let hist = $state<HistogramData | null>(null);
  let closing = false;
  let fallback = false;
  const session = new PreviewSession();
  const layerId = editor.summary?.active ?? null;

  onMount(async () => {
    hist = await histogram({ id: editor.active?.kind === "pixel" ? layerId : null });
  });

  async function apply() {
    const adjustment = $state.snapshot(value);
    try {
      await session.exec({ op: "filter.apply-adjustment", adjustment, ...(layerId != null ? { id: layerId } : {}) });
      fallback = false;
    } catch (e) {
      if (!isUnknownOp(e)) throw e;
      // Preview as an adjustment layer; OK merges it down.
      fallback = true;
      await session.exec({ op: "layer.add-adjustment", adjustment, above: layerId ?? undefined });
    }
  }

  let timer = 0;
  $effect(() => {
    const key = JSON.stringify(value);
    const on = preview;
    clearTimeout(timer);
    if (closing) return;
    if (!on) {
      void session.cancel();
      return;
    }
    timer = window.setTimeout(() => session.request(key, apply), 60);
  });
  onDestroy(() => clearTimeout(timer));

  async function ok() {
    closing = true;
    clearTimeout(timer);
    const key = JSON.stringify(value);
    const done = fallback ? (await session.cancel(), await applyAdjustment($state.snapshot(value))) : await session.commit(key, apply);
    if (done) pro.close();
    else closing = false;
  }

  async function cancel() {
    closing = true;
    clearTimeout(timer);
    await session.cancel();
    pro.close();
  }

  // Eyedroppers read the image without the preview, then the preview comes
  // back with whatever the dropper set.
  async function sampleOriginal(x: number, y: number) {
    clearTimeout(timer);
    await session.cancel();
    try {
      return await sampleComposite(x, y, 3);
    } finally {
      if (preview && !closing) session.request(JSON.stringify(value), apply);
    }
  }

  const label = $derived(kindInfo(adjKind)?.label ?? adjKind);
</script>

<Dialog title={adjKind === "develop" ? t("Camera Raw Filter") : label} width={adjKind === "develop" || adjKind === "curves" || adjKind === "levels" ? 360 : 330} scrim="clear" align="right" testid="adjustment-dialog" onclose={cancel} onsubmit={ok}>
  <AdjustmentEditor {value} histogram={hist} onchange={(next) => (value = next)} sample={sampleOriginal} />
  {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
  {#snippet footer()}
    <label class="ops-check preview"><input type="checkbox" bind:checked={preview} />{t("Preview")}</label>
    <button type="button" class="oa-btn oa-btn--secondary" onclick={cancel}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={ok}>{t("OK")}</button>
  {/snippet}
</Dialog>

<style>
  .preview {
    margin-right: auto;
  }
</style>
