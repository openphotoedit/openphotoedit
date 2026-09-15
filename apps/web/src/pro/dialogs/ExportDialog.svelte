<script lang="ts">
  // File › Export › Export As: format, quality, size and a target file size,
  // with a live estimate of the file that will be written.
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { exportBlob, saveBlob, type ExportFormat } from "../../lib/io";
  import Dialog from "../../ui/Dialog.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Slider from "../../ui/Slider.svelte";
  import { pro } from "../state.svelte";

  const s = editor.summary!;
  let format = $state<ExportFormat>("jpeg");
  let quality = $state(90);
  let scale = $state(100);
  let limit = $state(false);
  let maxKb = $state(500);
  let estimate = $state<{ bytes: number; w: number; h: number } | null>(null);
  let estimating = $state(false);
  let saving = $state(false);

  const width = $derived(Math.max(1, Math.round((s.width * scale) / 100)));
  const height = $derived(Math.max(1, Math.round((s.height * scale) / 100)));

  const opts = () => ({
    format,
    quality: quality / 100,
    width: scale !== 100 ? width : undefined,
    maxBytes: limit && format !== "png" ? maxKb * 1024 : undefined,
  });

  let timer = 0;
  $effect(() => {
    const o = opts();
    clearTimeout(timer);
    estimating = true;
    timer = window.setTimeout(async () => {
      try {
        const b = await exportBlob(o);
        estimate = { bytes: b.size, w: o.width ?? s.width, h: o.width ? Math.round((s.height * o.width) / s.width) : s.height };
      } catch {
        estimate = null;
      } finally {
        estimating = false;
      }
    }, 350);
    return () => clearTimeout(timer);
  });

  const fmtBytes = (b: number) => (b < 1024 * 1024 ? `${Math.round(b / 1024)} KB` : `${(b / 1048576).toFixed(2)} MB`);

  async function save() {
    saving = true;
    try {
      const blob = await exportBlob(opts());
      const ext = format === "jpeg" ? "jpg" : format;
      if (await saveBlob(blob, `${editor.fileName}.${ext}`)) {
        editor.dirty = false;
        pro.close();
      }
    } catch (e) {
      editor.error(e);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog title={t("Export As")} width={380} testid="export-dialog" onclose={() => pro.close()} onsubmit={save}>
  <div class="ops-stack">
    <div class="ops-row">
      <span class="ops-label lbl">{t("Format")}</span>
      <SegmentedControl
        ariaLabel={t("Format")}
        testid="export-format"
        options={[
          { value: "png", label: "PNG" },
          { value: "jpeg", label: "JPEG" },
          { value: "webp", label: "WebP" },
        ]}
        value={format}
        onchange={(f) => (format = f)}
      />
    </div>
    {#if format !== "png"}
      <Slider label={t("Quality")} value={quality} min={1} max={100} unit="%" defaultValue={90} disabled={limit} oninput={(v) => (quality = v)} />
      <div class="ops-row">
        <label class="ops-check"><input type="checkbox" bind:checked={limit} />{t("Limit file size to")}</label>
        <NumberField value={maxKb} min={10} max={100000} unit="KB" width={80} disabled={!limit} ariaLabel={t("Largest file size")} onchange={(v) => (maxKb = v)} />
      </div>
    {:else}
      <p class="ops-note">{t("PNG is lossless and keeps transparency.")}</p>
    {/if}
    <hr class="ops-hr" />
    <div class="ops-row">
      <span class="ops-label lbl">{t("Scale")}</span>
      <NumberField value={scale} min={1} max={400} unit="%" width={64} ariaLabel={t("Scale")} onchange={(v) => (scale = v)} />
      <span class="grow"></span>
      <NumberField label="W" value={width} min={1} max={s.width * 4} unit="px" width={76} onchange={(v) => (scale = Math.round((v / s.width) * 10000) / 100)} />
    </div>
    <p class="ops-note">{t("Output")}: {width} × {height} px</p>
    <hr class="ops-hr" />
    <div class="estimate">
      <span class="ops-label">{t("Estimated file size")}</span>
      <strong data-testid="export-estimate">{estimating && !estimate ? "…" : estimate ? fmtBytes(estimate.bytes) : "—"}</strong>
    </div>
    <p class="ops-note">{format === "jpeg" ? t("Transparent areas become white in JPEG.") : ""} {t("Camera metadata (EXIF, GPS location) is not written to exported files.")}</p>
  </div>
  {#snippet footer()}
    <button type="button" class="oa-btn oa-btn--secondary" onclick={() => pro.close()}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" disabled={saving} onclick={save}>{saving ? t("Exporting…") : t("Export")}</button>
  {/snippet}
</Dialog>

<style>
  .lbl {
    width: 64px;
  }
  .grow {
    flex: 1;
  }
  .estimate {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  strong {
    font: var(--weight-medium) var(--text-sm) / 1 var(--font-sans);
    color: var(--text-strong);
    font-variant-numeric: tabular-nums;
  }
</style>
