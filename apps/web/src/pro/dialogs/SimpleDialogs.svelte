<script lang="ts">
  // Small dialogs that share one shape: Fill, Rotate Canvas, Modify
  // Selection, Color Range, Select and Mask, the foreground/background
  // colour picker and smart filter blending options. Selection dialogs
  // preview on the canvas.
  import { onDestroy } from "svelte";
  import { BLEND_LABELS, BLEND_MODES, type BlendMode, type Rgba8 } from "../../engine/types";
  import { allLayers } from "../../engine/types";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import ColorSwatch from "../../ui/ColorSwatch.svelte";
  import Dialog from "../../ui/Dialog.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";
  import Slider from "../../ui/Slider.svelte";
  import { PreviewSession, run } from "../engine.svelte";
  import { pro, type DialogSpec } from "../state.svelte";
  import type { ProLayer } from "../types";

  let { spec }: { spec: DialogSpec } = $props();

  const close = () => pro.close();

  // ---- Fill
  let fillWith = $state<"foreground" | "background" | "black" | "white" | "gray" | "color">("foreground");
  let fillColor = $state<Rgba8>({ ...editor.primary });
  let fillOpacity = $state(100);
  const fillResolved = $derived(
    fillWith === "foreground" ? editor.primary : fillWith === "background" ? editor.secondary : fillWith === "black" ? { r: 0, g: 0, b: 0, a: 255 } : fillWith === "white" ? { r: 255, g: 255, b: 255, a: 255 } : fillWith === "gray" ? { r: 128, g: 128, b: 128, a: 255 } : fillColor,
  );
  async function doFill() {
    close();
    await run({ op: "layer.fill-selection", color: $state.snapshot(fillResolved), opacity: fillOpacity / 100 }, undefined, t("Fill"));
  }

  // ---- Rotate
  let degrees = $state(15);
  let clockwise = $state(true);
  let expand = $state(true);
  async function doRotate() {
    close();
    await run({ op: "image.rotate-arbitrary", degrees: clockwise ? degrees : -degrees, expand }, undefined, t("Rotate canvas"));
  }

  // ---- Selection previews
  const session = new PreviewSession();
  let closing = false;
  let timer = 0;
  onDestroy(() => clearTimeout(timer));

  function schedule(key: string, cmd: () => Record<string, unknown>) {
    clearTimeout(timer);
    if (closing) return;
    timer = window.setTimeout(() => session.request(key, () => session.exec(cmd())), 120);
  }
  async function commit(key: string, cmd: () => Record<string, unknown>) {
    closing = true;
    clearTimeout(timer);
    if (await session.commit(key, () => session.exec(cmd()))) close();
    else closing = false;
  }
  async function cancelPreview() {
    closing = true;
    clearTimeout(timer);
    await session.cancel();
    close();
  }

  // Modify
  const MODIFY = {
    grow: { title: t("Expand Selection"), label: t("Expand by"), key: "by", op: "select.grow", max: 500, def: 4 },
    shrink: { title: t("Contract Selection"), label: t("Contract by"), key: "by", op: "select.shrink", max: 500, def: 4 },
    border: { title: t("Border Selection"), label: t("Width"), key: "width", op: "select.border", max: 200, def: 8 },
    smooth: { title: t("Smooth Selection"), label: t("Sample radius"), key: "radius", op: "select.smooth", max: 500, def: 4 },
    feather: { title: t("Feather Selection"), label: t("Feather radius"), key: "radius", op: "select.feather", max: 1000, def: 8 },
  } as const;
  const mod = $derived(spec.kind === "modify" ? MODIFY[spec.op] : null);
  let modValue = $state<number>((() => (spec.kind === "modify" ? MODIFY[spec.op].def : 4))());
  const modCmd = () => ({ op: mod!.op, [mod!.key]: modValue });
  $effect(() => {
    if (spec.kind === "modify") schedule(`m${modValue}`, modCmd);
  });

  // Color range
  let crSelect = $state<"sampled" | "highlights" | "midtones" | "shadows" | "skin">("sampled");
  let crColor = $state<Rgba8>({ ...editor.primary });
  let fuzziness = $state(40);
  let crMode = $state<"replace" | "add" | "subtract" | "intersect">("replace");
  const crCmd = () => ({ op: "select.color-range", ...(crSelect === "sampled" ? { color: $state.snapshot(crColor) } : { preset: crSelect === "skin" ? "skin" : crSelect }), fuzziness, mode: crMode });
  $effect(() => {
    if (spec.kind === "color-range") schedule(JSON.stringify(crCmd()), crCmd);
  });

  // Select and Mask
  let refine = $state({ radius: 4, smooth: 0, feather: 0, contrast: 0, shift_edge: 0, decontaminate: false });
  const refineCmd = () => ({ op: "select.refine", ...$state.snapshot(refine) });
  $effect(() => {
    if (spec.kind === "select-mask") schedule(JSON.stringify(refine), refineCmd);
  });

  // ---- Colour picker
  const which = (() => (spec.kind === "color" ? spec.which : "primary"))();
  const original = which === "primary" ? { ...editor.primary } : { ...editor.secondary };
  let picked = $state<Rgba8>({ ...original });

  // ---- Smart filter blending
  const sf = (() => (spec.kind === "smart-filter-blend" ? spec : null))();
  const sfLayer = sf ? (allLayers(editor.summary?.layers ?? []).find((l) => l.id === sf.id) as unknown as ProLayer | undefined) : undefined;
  const sfFilter = sf ? sfLayer?.smart?.filters[sf.index] : undefined;
  let sfOpacity = $state(Math.round((sfFilter?.opacity ?? 1) * 100));
  let sfBlend = $state<BlendMode>(sfFilter?.blend ?? "normal");
  $effect(() => {
    if (sf) schedule(`${sfOpacity}:${sfBlend}`, () => ({ op: "layer.smart-filter-set", id: sf.id, index: sf.index, opacity: sfOpacity / 100, blend: sfBlend }));
  });
  const blendOptions = BLEND_MODES.map((m) => (m ? { value: m, label: t(BLEND_LABELS[m]) } : null));
</script>

{#if spec.kind === "fill"}
  <Dialog title={t("Fill")} width={320} testid="fill-dialog" onclose={close} onsubmit={doFill}>
    <div class="ops-stack">
      <div class="ops-row">
        <span class="ops-label lbl">{t("Contents")}</span>
        <Select
          ariaLabel={t("Contents")}
          value={fillWith}
          width={150}
          options={[
            { value: "foreground", label: t("Foreground colour") },
            { value: "background", label: t("Background colour") },
            { value: "color", label: t("Colour…") },
            null,
            { value: "black", label: t("Black") },
            { value: "gray", label: t("50% Gray") },
            { value: "white", label: t("White") },
          ]}
          onchange={(v) => (fillWith = v)}
        />
        <ColorSwatch color={fillResolved} size={20} />
      </div>
      {#if fillWith === "color"}
        <ColorPicker value={fillColor} height={100} oninput={(c) => (fillColor = c)} />
      {/if}
      <Slider label={t("Opacity")} value={fillOpacity} min={0} max={100} unit="%" defaultValue={100} oninput={(v) => (fillOpacity = v)} />
      <p class="ops-note">{editor.summary?.selection ? t("Fills the selection on the active layer.") : t("No selection: fills the whole active layer.")}</p>
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={close}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={doFill}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "rotate"}
  <Dialog title={t("Rotate Canvas")} width={300} onclose={close} onsubmit={doRotate}>
    <div class="ops-stack">
      <div class="ops-row">
        <span class="ops-label lbl">{t("Angle")}</span>
        <NumberField value={degrees} min={-359.99} max={359.99} step={0.01} unit="°" width={80} ariaLabel={t("Angle")} onchange={(v) => (degrees = v)} />
      </div>
      <SegmentedControl
        ariaLabel={t("Direction")}
        options={[
          { value: "cw", label: t("Clockwise") },
          { value: "ccw", label: t("Counter clockwise") },
        ]}
        value={clockwise ? "cw" : "ccw"}
        onchange={(v) => (clockwise = v === "cw")}
      />
      <label class="ops-check"><input type="checkbox" bind:checked={expand} />{t("Enlarge the canvas to fit")}</label>
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={close}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={doRotate}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "modify" && mod}
  <Dialog title={mod.title} width={300} scrim="clear" align="right" onclose={cancelPreview} onsubmit={() => commit(`m${modValue}`, modCmd)}>
    <div class="ops-stack">
      <div class="ops-row">
        <span class="ops-label grow">{mod.label}</span>
        <NumberField value={modValue} min={spec.op === "feather" ? 0.1 : 1} max={mod.max} step={spec.op === "feather" ? 0.1 : 1} unit="px" width={76} ariaLabel={mod.label} onchange={(v) => (modValue = v)} />
      </div>
      {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={cancelPreview}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={() => commit(`m${modValue}`, modCmd)}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "color-range"}
  <Dialog title={t("Color Range")} width={320} scrim="clear" align="right" testid="color-range-dialog" onclose={cancelPreview} onsubmit={() => commit(JSON.stringify(crCmd()), crCmd)}>
    <div class="ops-stack">
      <div class="ops-row">
        <span class="ops-label lbl">{t("Select")}</span>
        <Select
          ariaLabel={t("Select")}
          value={crSelect}
          width={150}
          options={[
            { value: "sampled", label: t("Sampled colours") },
            null,
            { value: "highlights", label: t("Highlights") },
            { value: "midtones", label: t("Midtones") },
            { value: "shadows", label: t("Shadows") },
            null,
            { value: "skin", label: t("Skin tones") },
          ]}
          onchange={(v) => (crSelect = v)}
        />
      </div>
      {#if crSelect === "sampled"}
        <div class="ops-row">
          <span class="ops-label lbl">{t("Colour")}</span>
          <ColorSwatch color={crColor} size={20} />
          <button type="button" class="oa-btn oa-btn--ghost small" onclick={() => (crColor = { ...editor.primary })}>{t("Use foreground")}</button>
        </div>
        <ColorPicker value={crColor} height={70} showFields={false} onchange={(c) => (crColor = c)} />
      {/if}
      <Slider label={t("Fuzziness")} value={fuzziness} min={0} max={200} defaultValue={40} oninput={(v) => (fuzziness = v)} />
      <SegmentedControl
        ariaLabel={t("Selection mode")}
        size="xs"
        options={[
          { value: "replace", label: t("New") },
          { value: "add", label: t("Add") },
          { value: "subtract", label: t("Subtract") },
          { value: "intersect", label: t("Intersect") },
        ]}
        value={crMode}
        onchange={(v) => (crMode = v)}
      />
      {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={cancelPreview}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={() => commit(JSON.stringify(crCmd()), crCmd)}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "select-mask"}
  <Dialog title={t("Select and Mask")} width={320} scrim="clear" align="right" testid="select-mask-dialog" onclose={cancelPreview} onsubmit={() => commit(JSON.stringify(refine), refineCmd)}>
    <div class="ops-stack">
      <span class="ops-section-title">{t("Edge detection")}</span>
      <Slider label={t("Radius")} value={refine.radius} min={0} max={250} unit="px" defaultValue={4} oninput={(v) => (refine.radius = v)} />
      <span class="ops-section-title">{t("Global refinements")}</span>
      <Slider label={t("Smooth")} value={refine.smooth} min={0} max={100} defaultValue={0} oninput={(v) => (refine.smooth = v)} />
      <Slider label={t("Feather")} value={refine.feather} min={0} max={250} step={0.1} unit="px" defaultValue={0} oninput={(v) => (refine.feather = v)} />
      <Slider label={t("Contrast")} value={refine.contrast} min={0} max={100} unit="%" defaultValue={0} oninput={(v) => (refine.contrast = v)} />
      <Slider label={t("Shift edge")} value={refine.shift_edge} min={-100} max={100} unit="%" defaultValue={0} oninput={(v) => (refine.shift_edge = v)} />
      <span class="ops-section-title">{t("Output")}</span>
      <label class="ops-check"><input type="checkbox" bind:checked={refine.decontaminate} />{t("Decontaminate colours")}</label>
      {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={cancelPreview}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={() => commit(JSON.stringify(refine), refineCmd)}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "color"}
  <Dialog
    title={which === "primary" ? t("Color Picker (Foreground Color)") : t("Color Picker (Background Color)")}
    width={340}
    testid="color-dialog"
    onclose={() => {
      if (which === "primary") editor.primary = original;
      else editor.secondary = original;
      close();
    }}
    onsubmit={close}
  >
    <div class="ops-stack">
      <ColorPicker
        value={picked}
        height={200}
        oninput={(c) => {
          picked = c;
          if (which === "primary") editor.primary = c;
          else editor.secondary = c;
        }}
      />
      <div class="ops-row compare">
        <span class="ops-label">{t("New")}</span>
        <ColorSwatch color={picked} size={28} />
        <ColorSwatch color={original} size={28} label={t("Current")} />
        <span class="ops-label">{t("Current")}</span>
      </div>
    </div>
    {#snippet footer()}
      <button
        type="button"
        class="oa-btn oa-btn--secondary"
        onclick={() => {
          if (which === "primary") editor.primary = original;
          else editor.secondary = original;
          close();
        }}>{t("Cancel")}</button
      >
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={close}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{:else if sf}
  <Dialog title={t("Blending Options")} width={300} scrim="clear" align="right" onclose={cancelPreview} onsubmit={() => commit(`${sfOpacity}:${sfBlend}`, () => ({ op: "layer.smart-filter-set", id: sf.id, index: sf.index, opacity: sfOpacity / 100, blend: sfBlend }))}>
    <div class="ops-stack">
      <div class="ops-row">
        <span class="ops-label lbl">{t("Mode")}</span>
        <Select ariaLabel={t("Blend mode")} value={sfBlend} width={150} options={blendOptions} onchange={(v) => (sfBlend = v as BlendMode)} />
      </div>
      <Slider label={t("Opacity")} value={sfOpacity} min={0} max={100} unit="%" defaultValue={100} oninput={(v) => (sfOpacity = v)} />
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={cancelPreview}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={() => commit(`${sfOpacity}:${sfBlend}`, () => ({ op: "layer.smart-filter-set", id: sf.id, index: sf.index, opacity: sfOpacity / 100, blend: sfBlend }))}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .lbl {
    width: 72px;
  }
  .grow {
    flex: 1;
  }
  .small {
    height: 22px;
    padding: 0 8px;
    font-size: var(--text-xs);
  }
  .compare {
    justify-content: center;
    gap: 6px;
  }
</style>
