<script lang="ts">
  // Export: format, quality, size, an optional file-size limit, and the
  // real size of the result before saving (the encode is done once and
  // reused by Save).
  import Download from "@lucide/svelte/icons/download";
  import Copy from "@lucide/svelte/icons/copy";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { exportBlob, saveBlob, type ExportFormat } from "../lib/io";
  import Sheet from "./components/Sheet.svelte";
  import Slider from "./components/Slider.svelte";
  import { lite } from "./lite.svelte";

  let format = $state<ExportFormat>("jpeg");
  let quality = $state(90);
  let size = $state<"original" | "2048" | "1080">("original");
  let limit = $state<"none" | "500" | "1000">("none");

  let result = $state<{ key: string; blob: Blob } | null>(null);
  let encoding = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const s = $derived(editor.summary);
  const longEdge = $derived(s ? Math.max(s.width, s.height) : 0);

  function outSize(): { w: number; h: number; width?: number } {
    if (!s) return { w: 0, h: 0 };
    const target = size === "original" ? longEdge : Number(size);
    if (target >= longEdge) return { w: s.width, h: s.height };
    const k = target / longEdge;
    const w = Math.round(s.width * k);
    return { w, h: Math.round(s.height * k), width: w };
  }
  const dims = $derived.by(() => {
    void size;
    void s?.width;
    void s?.height;
    return outSize();
  });

  const key = $derived(`${s?.revision}|${format}|${quality}|${size}|${limit}|${s?.width}x${s?.height}`);
  const lossy = $derived(format !== "png");
  const ext = $derived(format === "jpeg" ? "jpg" : format);

  async function encode(k: string): Promise<Blob | null> {
    if (result?.key === k) return result.blob;
    const o = outSize();
    const blob = await exportBlob({
      format,
      quality: quality / 100,
      width: o.width,
      maxBytes: lossy && limit !== "none" ? Number(limit) * 1000 : undefined,
    });
    if (k === key) result = { key: k, blob };
    return blob;
  }

  $effect(() => {
    const k = key;
    if (!lite.exportOpen || !editor.hasDocument) return;
    const timer = setTimeout(async () => {
      encoding = true;
      error = null;
      try {
        await lite.lock.run(() => encode(k));
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      } finally {
        encoding = false;
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  function fmtBytes(n: number) {
    if (n < 1000) return `${n} B`;
    if (n < 1_000_000) return `${Math.round(n / 1000)} KB`;
    return `${(n / 1_000_000).toFixed(1)} MB`;
  }

  async function save() {
    saving = true;
    try {
      const blob = await lite.lock.run(() => encode(key));
      if (!blob) return;
      const ok = await saveBlob(blob, `${editor.fileName}.${ext}`);
      if (ok) {
        editor.dirty = false;
        lite.exportOpen = false;
        editor.toast(t("Saved {name} ({size})", { name: `${editor.fileName}.${ext}`, size: fmtBytes(blob.size) }), "success");
      }
    } catch (e) {
      editor.error(e);
    } finally {
      saving = false;
    }
  }

  async function copy() {
    try {
      const blob = await lite.lock.run(() => exportBlob({ format: "png", width: outSize().width }));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      editor.toast(t("Copied to the clipboard"), "success");
    } catch (e) {
      editor.toast(t("This browser would not copy the image. Save it instead."), "error");
      console.error(e);
    }
  }

  const FORMATS: { id: ExportFormat; label: string; hint: string }[] = [
    { id: "jpeg", label: "JPEG", hint: "Best for photos. Smallest files." },
    { id: "png", label: "PNG", hint: "Lossless, keeps transparency. Larger files." },
    { id: "webp", label: "WebP", hint: "Small files with transparency, for the web." },
  ];
</script>

<Sheet title={t("Export")} open={lite.exportOpen} onclose={() => (lite.exportOpen = false)} testid="export-sheet" width={440}>
  <div class="form">
    <div class="field">
      <span class="lt-eyebrow">{t("Format")}</span>
      <div class="lt-seg" role="group" aria-label={t("Format")}>
        {#each FORMATS as f (f.id)}
          <button aria-pressed={format === f.id} data-testid="format-{f.id}" onclick={() => (format = f.id)}>{f.label}</button>
        {/each}
      </div>
      <p class="lt-hint">{t(FORMATS.find((f) => f.id === format)!.hint)}</p>
    </div>

    <div class="field">
      <span class="lt-eyebrow">{t("Size")}</span>
      <div class="lt-seg" role="group" aria-label={t("Size")}>
        <button aria-pressed={size === "original"} onclick={() => (size = "original")}>{t("Original")}</button>
        <button aria-pressed={size === "2048"} disabled={longEdge <= 2048} onclick={() => (size = "2048")}>2048 px</button>
        <button aria-pressed={size === "1080"} disabled={longEdge <= 1080} onclick={() => (size = "1080")}>{t("1080 px")}</button>
      </div>
      <p class="lt-hint">{dims.w} × {dims.h} px{size === "1080" ? ` · ${t("right for Instagram")}` : ""}</p>
    </div>

    {#if lossy}
      <Slider label={t("Quality")} value={quality} min={40} max={100} defaultValue={90} format={(v) => `${Math.round(v)}`} disabled={limit !== "none"} oninput={(v) => (quality = v)} />
      <div class="field">
        <span class="lt-eyebrow">{t("File size limit")}</span>
        <div class="lt-seg" role="group" aria-label={t("File size limit")}>
          <button aria-pressed={limit === "none"} onclick={() => (limit = "none")}>{t("No limit")}</button>
          <button aria-pressed={limit === "500"} data-testid="limit-500" onclick={() => (limit = "500")}>{t("Under 500 KB")}</button>
          <button aria-pressed={limit === "1000"} onclick={() => (limit = "1000")}>{t("Under 1 MB")}</button>
        </div>
      </div>
    {/if}

    <p class="note"><ShieldCheck size={14} />{t("Saved as a fresh copy: location and camera details from the original are not included.")}</p>
  </div>

  {#snippet footer()}
    <span class="estimate" data-testid="export-estimate" aria-live="polite">
      {#if encoding}
        <LoaderCircle size={14} class="lt-spin" />{t("Working out the size…")}
      {:else if error}
        {error}
      {:else if result?.key === key}
        {fmtBytes(result.blob.size)}
      {/if}
    </span>
    <button class="oa-btn oa-btn--secondary oa-btn--md" onclick={copy}><Copy size={16} />{t("Copy")}</button>
    <button class="oa-btn oa-btn--primary oa-btn--md" data-testid="export-save" data-autofocus disabled={saving || !editor.hasDocument} onclick={save}>
      {#if saving}<LoaderCircle size={16} class="lt-spin" />{:else}<Download size={16} />{/if}
      {t("Save")}
    </button>
  {/snippet}
</Sheet>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    font: var(--type-caption);
    color: var(--text-faint);
  }
  .note :global(svg) {
    flex: 0 0 auto;
    margin-top: 1px;
  }
  .estimate {
    margin-right: auto;
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--type-mono);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
