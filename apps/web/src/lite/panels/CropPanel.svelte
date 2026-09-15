<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import FlipHorizontal2 from "@lucide/svelte/icons/flip-horizontal-2";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Slider from "../components/Slider.svelte";
  import { lite } from "../lite.svelte";
  import { flip, RATIOS, RESIZE_PRESETS, resizeFor, rotate, straighten, straightenedAngle } from "../ops";
  import { applyCropRatio, commitCrop, resetCrop } from "../../tools/crop";
  import { toolSettings, type CropRatio } from "../../tools/settings.svelte";
  import { toolState } from "../../tools/state.svelte";

  let angle = $state<number | null>(null);

  const ratioId = $derived.by(() => {
    const r = toolSettings.cropRatio;
    if (r === "custom") return `${toolSettings.cropCustomW}:${toolSettings.cropCustomH}`;
    return r;
  });
  const shown = $derived(angle ?? straightenedAngle());
  const pending = $derived(toolState.cropPending);
  const info = $derived(toolState.cropInfo);

  function pickRatio(id: string) {
    const known: CropRatio[] = ["free", "original", "1:1", "4:5", "3:2", "16:9", "9:16"];
    if ((known as string[]).includes(id)) {
      toolSettings.cropRatio = id as CropRatio;
    } else {
      const [w, h] = id.split(":").map(Number);
      toolSettings.cropCustomW = w;
      toolSettings.cropCustomH = h;
      toolSettings.cropRatio = "custom";
    }
    if (id === "free") resetCrop(editor);
    else applyCropRatio(editor);
  }

  async function apply() {
    const id = ratioId;
    const before = editor.summary?.history.undo.length ?? 0;
    lite.begin();
    await lite.lock.run(() => commitCrop(editor));
    const label = RATIOS.find((x) => x.id === id);
    if ((editor.summary?.history.undo.length ?? 0) > before) lite.note(id === "free" || !label ? t("Crop") : t("Crop to {ratio}", { ratio: t(label.label) }), /Crop/i);
  }

  function preview(v: number) {
    angle = v;
    lite.straightenPreview = v - straightenedAngle();
  }

  async function commitAngle() {
    const v = angle;
    if (v == null) return;
    await straighten(Math.round(v * 10) / 10);
    angle = null;
    lite.straightenPreview = null;
  }
</script>

<div class="crop">
  <section class="lt-section">
    <div class="lt-section__head">
      <h3 class="lt-eyebrow">{t("Aspect ratio")}</h3>
      {#if info.w}
        <span class="dims" data-testid="crop-dims">{info.w} × {info.h}</span>
      {/if}
    </div>
    <div class="lt-chips" class:lt-chips--scroll={lite.phone}>
      {#each RATIOS as x (x.id)}
        <button class="lt-chip" aria-pressed={ratioId === x.id} data-testid="ratio-{x.id}" onclick={() => pickRatio(x.id)}>{t(x.label)}</button>
      {/each}
    </div>
    <p class="lt-hint">{t("Drag the corners to frame the photo. Drag outside the frame to rotate it.")}</p>
    <div class="actions">
      <button class="oa-btn oa-btn--ghost oa-btn--md" disabled={!pending} onclick={() => { toolSettings.cropRatio = "free"; resetCrop(editor); }}>
        <Undo2 size={16} />
        {t("Reset")}
      </button>
      <button class="oa-btn oa-btn--primary oa-btn--md" disabled={!pending} data-testid="crop-apply" onclick={apply}>
        <Check size={16} />
        {t("Apply crop")}
      </button>
    </div>
  </section>

  <section class="lt-section">
    <h3 class="lt-eyebrow">{t("Straighten and rotate")}</h3>
    <Slider
      label={t("Straighten")}
      value={shown}
      min={-45}
      max={45}
      step={0.1}
      format={(v) => `${v > 0 ? "+" : ""}${v.toFixed(1)}°`}
      disabled={!editor.hasDocument}
      testid="slider-straighten"
      oninput={preview}
      oncommit={commitAngle}
    />
    <div class="lt-seg">
      <button onclick={() => rotate(3)}><RotateCcw size={16} />{t("Left")}</button>
      <button onclick={() => rotate(1)}><RotateCw size={16} />{t("Right")}</button>
      <button onclick={() => flip(true)}><FlipHorizontal2 size={16} />{t("Flip")}</button>
    </div>
  </section>

  <section class="lt-section">
    <h3 class="lt-eyebrow">{t("Resize for")}</h3>
    <ul class="presets">
      {#each RESIZE_PRESETS as p (p.id)}
        <li>
          <button class="preset" data-testid="resize-{p.id}" onclick={() => resizeFor(p)}>
            <span class="name">{t(p.label)}</span>
            <span class="detail">{t(p.detail)}</span>
          </button>
        </li>
      {/each}
    </ul>
    <p class="lt-hint">{t("Crops around the centre, then resizes.")}</p>
  </section>
</div>

<style>
  .crop {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .dims {
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .presets {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .presets li + li {
    border-top: var(--border-width) solid var(--border-hairline);
  }
  .preset {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: var(--touch-min);
    padding: 0 var(--space-3);
    border: 0;
    background: var(--surface-card);
    cursor: pointer;
    text-align: left;
    transition: var(--transition-control);
  }
  .preset:hover {
    background: var(--surface-hover);
  }
  .name {
    font: var(--type-ui);
    color: var(--text-strong);
  }
  .detail {
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
  }
</style>
