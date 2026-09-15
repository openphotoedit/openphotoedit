<script lang="ts">
  // Image › Image Size: pixels or percent, linked proportions, resolution,
  // and the resampling method. Without resampling only the resolution changes.
  import Link2 from "@lucide/svelte/icons/link-2";
  import Unlink2 from "@lucide/svelte/icons/unlink-2";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Dialog from "../../ui/Dialog.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import Select from "../../ui/Select.svelte";
  import { run } from "../engine.svelte";
  import { pro } from "../state.svelte";

  const s = editor.summary!;
  let unit = $state<"px" | "percent">("px");
  let width = $state(s.width);
  let height = $state(s.height);
  let resolution = $state(s.resolution);
  let linked = $state(true);
  let resample = $state(true);
  let method = $state<"auto" | "bicubic" | "bilinear" | "nearest" | "lanczos">("auto");

  const ratio = s.width / s.height;
  const size = (w: number, h: number) => {
    const mb = (w * h * 4) / 1048576;
    return mb < 1 ? `${Math.round(mb * 1024)}K` : `${mb.toFixed(1)}M`;
  };

  function setW(v: number) {
    width = Math.max(1, Math.round(unit === "px" ? v : (s.width * v) / 100));
    if (linked) height = Math.max(1, Math.round(width / ratio));
  }
  function setH(v: number) {
    height = Math.max(1, Math.round(unit === "px" ? v : (s.height * v) / 100));
    if (linked) width = Math.max(1, Math.round(height * ratio));
  }
  function setRes(v: number) {
    if (!resample) {
      resolution = v;
      return;
    }
    // With resampling, pixel size follows print size at the new resolution.
    const k = v / resolution;
    resolution = v;
    width = Math.max(1, Math.round(width * k));
    height = Math.max(1, Math.round(height * k));
  }

  async function ok() {
    pro.close();
    if (resample && (width !== s.width || height !== s.height)) {
      const chosen = method === "auto" ? (width < s.width ? "bicubic" : "lanczos") : method;
      await run({ op: "image.resize", width, height, resample: chosen }, undefined, t("Image Size"));
    }
    if (resolution !== s.resolution) await run({ op: "doc.set-resolution", resolution }, undefined, t("Resolution"));
  }
</script>

<Dialog title={t("Image Size")} width={380} testid="image-size-dialog" onclose={() => pro.close()} onsubmit={ok}>
  <div class="ops-stack">
    <p class="summary">
      {t("Image size")}: <strong>{size(width, height)}</strong>
      {#if width !== s.width || height !== s.height}<span class="was">({t("was {size}", { size: size(s.width, s.height) })})</span>{/if}
    </p>
    <p class="summary">{t("Dimensions")}: <strong>{width} × {height} px</strong></p>
    <hr class="ops-hr" />
    <div class="fields">
      <div class="pair">
        <div class="ops-row">
          <span class="ops-label lbl">{t("Width")}</span>
          <NumberField value={unit === "px" ? width : Math.round((width / s.width) * 1000) / 10} min={unit === "px" ? 1 : 0.1} max={unit === "px" ? 300000 : 10000} step={unit === "px" ? 1 : 0.1} width={80} ariaLabel={t("Width")} disabled={!resample} testid="image-size-width" onchange={setW} />
        </div>
        <div class="ops-row">
          <span class="ops-label lbl">{t("Height")}</span>
          <NumberField value={unit === "px" ? height : Math.round((height / s.height) * 1000) / 10} min={unit === "px" ? 1 : 0.1} max={unit === "px" ? 300000 : 10000} step={unit === "px" ? 1 : 0.1} width={80} ariaLabel={t("Height")} disabled={!resample} testid="image-size-height" onchange={setH} />
        </div>
      </div>
      <div class="link">
        <span class="bracket"></span>
        <IconButton size="sm" label={linked ? t("Unlink width and height") : t("Constrain proportions")} pressed={linked} onclick={() => (linked = !linked)}>
          {#if linked}<Link2 size={13} />{:else}<Unlink2 size={13} />{/if}
        </IconButton>
        <span class="bracket"></span>
      </div>
      <Select
        ariaLabel={t("Units")}
        value={unit}
        width={92}
        options={[
          { value: "px", label: t("Pixels") },
          { value: "percent", label: t("Percent") },
        ]}
        onchange={(u) => (unit = u)}
      />
    </div>
    <div class="ops-row">
      <span class="ops-label lbl">{t("Resolution")}</span>
      <NumberField value={resolution} min={1} max={10000} step={1} unit="ppi" width={80} ariaLabel={t("Resolution")} onchange={setRes} />
    </div>
    <hr class="ops-hr" />
    <div class="ops-row">
      <label class="ops-check"><input type="checkbox" bind:checked={resample} />{t("Resample")}</label>
      <span class="grow"></span>
      <Select
        ariaLabel={t("Resampling method")}
        value={method}
        width={190}
        disabled={!resample}
        options={[
          { value: "auto", label: t("Automatic") },
          null,
          { value: "lanczos", label: t("Lanczos (enlargement)") },
          { value: "bicubic", label: t("Bicubic (smooth gradients)") },
          { value: "bilinear", label: t("Bilinear") },
          { value: "nearest", label: t("Nearest neighbour (hard edges)") },
        ]}
        onchange={(m) => (method = m)}
      />
    </div>
    {#if !resample}<p class="ops-note">{t("Without resampling, the pixels stay the same and only the print size changes.")}</p>{/if}
  </div>
  {#snippet footer()}
    <button
      type="button"
      class="oa-btn oa-btn--ghost reset"
      onclick={() => {
        width = s.width;
        height = s.height;
        resolution = s.resolution;
      }}>{t("Reset")}</button
    >
    <button type="button" class="oa-btn oa-btn--secondary" onclick={() => pro.close()}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={ok}>{t("OK")}</button>
  {/snippet}
</Dialog>

<style>
  .summary {
    margin: 0;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  strong {
    font-weight: var(--weight-medium);
    color: var(--text-strong);
  }
  .was {
    color: var(--text-faint);
  }
  .fields {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .pair {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    width: 72px;
  }
  .link {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin-right: 6px;
  }
  .bracket {
    width: 8px;
    height: 8px;
    border-right: 1px solid var(--border-strong);
  }
  .bracket:first-child {
    border-top: 1px solid var(--border-strong);
  }
  .bracket:last-child {
    border-bottom: 1px solid var(--border-strong);
  }
  .grow {
    flex: 1;
  }
  .reset {
    margin-right: auto;
  }
</style>
