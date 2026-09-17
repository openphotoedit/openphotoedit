<script lang="ts">
  // Camera Raw: develop a RAW file before it opens. The preview starts as
  // the camera's embedded JPEG, then switches to our own develop at preview
  // size, re-rendered as sliders move (latest settings win; one render at a
  // time, so a slow machine drops intermediate positions instead of queueing).
  import { onMount } from "svelte";
  import { t } from "./i18n";
  import Dialog from "../ui/Dialog.svelte";
  import Select from "../ui/Select.svelte";
  import Slider from "../ui/Slider.svelte";
  import { DEFAULT_RAW_PARAMS, type RawEngine, type RawImage, type RawInfo, type RawParams, type WbPreset } from "./raw-io";

  let { name, info, raw, ondone }: { name: string; info: RawInfo; raw: RawEngine; ondone: (p: RawParams | null) => void } = $props();

  // The dialog is mounted once per file; the as-shot white balance seeds the sliders.
  const initial = (): RawParams => ({ ...DEFAULT_RAW_PARAMS, temperature: Math.round(info.as_shot_wb.temperature), tint: Math.round(info.as_shot_wb.tint) });
  let params = $state<RawParams>(initial());
  let canvas: HTMLCanvasElement;
  let error = $state("");
  let rendering = $state(false);
  let firstMs = $state<number | null>(null);
  let lastMs = $state<number | null>(null);
  let closed = false;

  const PREVIEW_SIZE = 1600;

  const wbOptions: { value: WbPreset; label: string }[] = [
    { value: "as-shot", label: t("As shot") },
    { value: "auto", label: t("Auto") },
    { value: "daylight", label: t("Daylight") },
    { value: "cloudy", label: t("Cloudy") },
    { value: "shade", label: t("Shade") },
    { value: "tungsten", label: t("Tungsten") },
    { value: "fluorescent", label: t("Fluorescent") },
    { value: "flash", label: t("Flash") },
  ];
  const wbSelectOptions = $derived(params.wb === "custom" ? [...wbOptions, { value: "custom" as WbPreset, label: t("Custom") }] : wbOptions);

  function shutter(s: number) {
    if (s >= 0.3) return `${Number(s.toFixed(1))} s`;
    return `1/${Math.round(1 / s)} s`;
  }
  const cameraLine = $derived(
    [
      `${info.make} ${info.model}`.trim(),
      info.lens,
      info.iso ? `ISO ${info.iso}` : null,
      info.exposure ? shutter(info.exposure) : null,
      info.aperture ? `f/${Number(info.aperture.toFixed(1))}` : null,
      info.focal ? `${Math.round(info.focal)} mm` : null,
      `${info.width} × ${info.height}`,
    ]
      .filter(Boolean)
      .join(" · "),
  );

  function draw(img: ImageBitmap | RawImage) {
    if (!canvas) return;
    canvas.width = img.width;
    canvas.height = img.height;
    const g = canvas.getContext("2d")!;
    if ("rgba" in img) g.putImageData(new ImageData(img.rgba as Uint8ClampedArray<ArrayBuffer>, img.width, img.height), 0, 0);
    else {
      g.drawImage(img, 0, 0);
      img.close();
    }
  }

  // Latest-wins rendering.
  let wanted = "";
  let shown = "";
  async function pump() {
    if (rendering || closed) return;
    while (wanted !== shown && !closed) {
      const key = wanted;
      rendering = true;
      const started = performance.now();
      try {
        const img = await raw.develop(JSON.parse(key), PREVIEW_SIZE);
        if (closed) break;
        draw(img);
        shown = key;
        const ms = Math.round(performance.now() - started);
        if (firstMs === null) firstMs = ms;
        else lastMs = ms;
        error = "";
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        shown = key;
      } finally {
        rendering = false;
      }
    }
  }

  // Ask for the embedded JPEG before the first develop so the worker answers
  // it first (tens of milliseconds); our own develop replaces it when ready.
  // Props never change for a mounted dialog, so reading `raw` once is right.
  const engine = raw;
  engine
    .preview(1024)
    .then((img) => {
      if (!closed && firstMs === null) draw(img);
      else if (!("rgba" in img)) img.close();
    })
    .catch(() => {});

  $effect(() => {
    wanted = JSON.stringify(params);
    void pump();
  });

  onMount(() => () => {
    closed = true;
  });

  async function setWb(v: WbPreset) {
    params.wb = v;
    if (v === "custom") return;
    try {
      const wb = await raw.whiteBalance($state.snapshot(params));
      if (params.wb === v) {
        params.temperature = Math.round(wb.temperature);
        params.tint = Math.round(wb.tint);
      }
    } catch {
      // The preview reports errors; the sliders keep their values.
    }
  }

  function setTemp(v: number) {
    params.temperature = v;
    params.wb = "custom";
  }
  function setTint(v: number) {
    params.tint = v;
    params.wb = "custom";
  }

  function reset() {
    params = initial();
  }

  function finish(result: RawParams | null) {
    closed = true;
    ondone(result);
  }

  // Kelvin slider track: warm to cool, like Camera Raw.
  const tempTrack = "linear-gradient(to right, #3f6fd8, #e8e8e8, #e0a33a)";
  const tintTrack = "linear-gradient(to right, #4caf50, #e8e8e8, #c04fc0)";
</script>

<Dialog title={t("Camera Raw: {name}", { name })} width={1040} testid="raw-dialog" onclose={() => finish(null)} onsubmit={() => finish($state.snapshot(params))}>
  <div class="raw">
    <div class="stage">
      <canvas bind:this={canvas} data-testid="raw-preview" aria-label={t("Preview")}></canvas>
      <div class="badge" aria-live="polite">
        {#if firstMs === null && !error}
          <span class="spinner"></span>{t("Developing preview…")}
        {:else if rendering}
          <span class="spinner"></span>{t("Updating…")}
        {/if}
      </div>
    </div>
    <div class="panel ops-stack">
      <p class="camera" data-testid="raw-camera">{cameraLine}</p>

      <h3 class="ops-section-title">{t("White balance")}</h3>
      <div class="row">
        <span class="ops-label grow">{t("Preset")}</span>
        <Select ariaLabel={t("White balance")} options={wbSelectOptions} value={params.wb} width={130} testid="raw-wb" onchange={setWb} />
      </div>
      <Slider label={t("Temperature")} value={params.temperature} min={2000} max={15000} step={50} unit="K" track={tempTrack} fieldWidth={62} testid="raw-temperature" oninput={setTemp} />
      <Slider label={t("Tint")} value={params.tint} min={-150} max={150} step={1} track={tintTrack} fieldWidth={62} testid="raw-tint" oninput={setTint} />

      <h3 class="ops-section-title">{t("Tone")}</h3>
      <Slider label={t("Exposure")} value={params.exposure} min={-5} max={5} step={0.05} precision={2} defaultValue={0} fieldWidth={62} testid="raw-exposure" oninput={(v) => (params.exposure = v)} />
      <Slider label={t("Contrast")} value={params.contrast} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-contrast" oninput={(v) => (params.contrast = v)} />
      <Slider label={t("Highlights")} value={params.highlights} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-highlights" oninput={(v) => (params.highlights = v)} />
      <Slider label={t("Shadows")} value={params.shadows} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-shadows" oninput={(v) => (params.shadows = v)} />
      <Slider label={t("Whites")} value={params.whites} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-whites" oninput={(v) => (params.whites = v)} />
      <Slider label={t("Blacks")} value={params.blacks} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-blacks" oninput={(v) => (params.blacks = v)} />

      <h3 class="ops-section-title">{t("Color")}</h3>
      <Slider label={t("Vibrance")} value={params.vibrance} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-vibrance" oninput={(v) => (params.vibrance = v)} />
      <Slider label={t("Saturation")} value={params.saturation} min={-100} max={100} defaultValue={0} fieldWidth={62} testid="raw-saturation" oninput={(v) => (params.saturation = v)} />

      <h3 class="ops-section-title">{t("Detail")}</h3>
      <Slider label={t("Sharpening")} value={params.sharpen} min={0} max={150} defaultValue={DEFAULT_RAW_PARAMS.sharpen} fieldWidth={62} testid="raw-sharpen" oninput={(v) => (params.sharpen = v)} />
      <Slider label={t("Noise reduction")} value={params.noise_luma} min={0} max={100} defaultValue={0} fieldWidth={62} testid="raw-noise-luma" oninput={(v) => (params.noise_luma = v)} />
      <Slider label={t("Color noise reduction")} value={params.noise_chroma} min={0} max={100} defaultValue={DEFAULT_RAW_PARAMS.noise_chroma} fieldWidth={62} testid="raw-noise-chroma" oninput={(v) => (params.noise_chroma = v)} />
      <p class="note">{t("Sharpening and noise reduction are previewed at reduced size; check them at 100% after opening.")}</p>

      {#if error}<p class="ops-error" role="alert">{t("The preview failed: {reason}", { reason: error })}</p>{/if}
    </div>
  </div>
  {#snippet footer()}
    <span class="timing" data-testid="raw-timing">
      {#if firstMs !== null}{t("First preview {ms} ms", { ms: firstMs })}{#if lastMs !== null} · {t("last update {ms} ms", { ms: lastMs })}{/if}{/if}
    </span>
    <button type="button" class="oa-btn oa-btn--ghost" onclick={reset}>{t("Reset")}</button>
    <button type="button" class="oa-btn oa-btn--secondary" onclick={() => finish(null)}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="raw-open" onclick={() => finish($state.snapshot(params))}>{t("Open image")}</button>
  {/snippet}
</Dialog>

<style>
  .raw {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 280px;
    gap: var(--space-4);
    height: min(640px, calc(100dvh - 160px));
  }
  .stage {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 0;
    background: var(--bg-sunken);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
    overflow: hidden;
  }
  canvas {
    max-width: 100%;
    max-height: 100%;
    display: block;
  }
  .badge {
    position: absolute;
    left: var(--space-2);
    bottom: var(--space-2);
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font: var(--type-caption);
  }
  .panel {
    min-height: 0;
    overflow-y: auto;
    padding-right: var(--space-1);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .camera {
    margin: 0;
    color: var(--text-strong);
    font: var(--type-caption);
    line-height: 1.4;
  }
  h3 {
    margin: var(--space-2) 0 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  .note {
    margin: 0;
    color: var(--text-muted);
    font: var(--type-caption);
  }
  .timing {
    margin-right: auto;
    color: var(--text-muted);
    font: var(--type-caption);
  }
  .spinner {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong);
    border-top-color: var(--text-strong);
    animation: oa-spin 0.8s linear infinite;
  }
</style>
