<script lang="ts">
  // Properties for whatever is active: an adjustment layer's full editor
  // (drags send `layer.set-adjustment` once per frame and seal on release),
  // position and size for pixel layers, text and shape parameters, fill
  // colours, smart object details, the mask, or the document itself.
  import Image from "@lucide/svelte/icons/image";
  import SquareDashed from "@lucide/svelte/icons/square-dashed";
  import type { Adjustment, Rgba8, ShapeData, TextData } from "../../engine/types";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import ColorSwatch from "../../ui/ColorSwatch.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Slider from "../../ui/Slider.svelte";
  import type { HistogramData } from "../../ui/histogram";
  import AdjustmentEditor from "../adjust/AdjustmentEditor.svelte";
  import { sampleComposite } from "../adjust/canvas-pick";
  import { kindInfo } from "../adjustments";
  import { histogram, rafThrottle, run } from "../engine.svelte";
  import { ACTIONS } from "../actions.svelte";
  import { replaceSmartContents, unpackSmart } from "../layer-ops";
  import { pro } from "../state.svelte";
  import type { ProLayer } from "../types";

  const active = $derived(editor.hasDocument ? (editor.active as ProLayer | null) : null);
  const s = $derived(editor.summary);

  // ---- Adjustment draft
  let draft = $state<Adjustment | null>(null);
  let draftFor = -1;
  let draftRev = -1;
  let dragging = false;
  let hist = $state<HistogramData | null>(null);

  $effect(() => {
    const a = active;
    if (!a || a.kind !== "adjustment" || !a.adjustment) {
      draft = null;
      draftFor = -1;
      return;
    }
    if (a.id !== draftFor || (a.rev !== draftRev && !dragging)) {
      draft = structuredClone($state.snapshot(a.adjustment));
      draftFor = a.id;
      draftRev = a.rev;
    }
  });

  // Histograms show what the adjustment receives: the composite, refreshed when idle.
  $effect(() => {
    const kind = active?.adjustment?.kind;
    void s?.revision;
    if (kind !== "levels" && kind !== "curves" && kind !== "threshold") return;
    const timer = setTimeout(async () => {
      if (!dragging) hist = await histogram({});
    }, 250);
    return () => clearTimeout(timer);
  });

  const sendAdjustment = rafThrottle(async (id: number, adjustment: Adjustment) => {
    await run({ op: "layer.set-adjustment", id, adjustment }, undefined, t("Adjustment"));
  });

  async function onAdjust(next: Adjustment, final: boolean) {
    const a = active;
    if (!a) return;
    draft = next;
    dragging = !final;
    sendAdjustment(a.id, next);
    if (final) {
      await sendAdjustment.flush();
      await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
      draftRev = -1;
    }
  }

  // ---- Generic throttled sends for text, shape, fill, mask density
  const sendRaw = rafThrottle(async (cmd: Record<string, unknown>) => {
    await run(cmd);
  });
  async function seal() {
    await sendRaw.flush();
    await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
  }

  let maskDensity = $state<number | null>(null);

  // ---- Position
  function moveTo(axis: "x" | "y", value: number) {
    const a = active;
    if (!a?.bounds) return;
    const dx = axis === "x" ? value - a.bounds.x : 0;
    const dy = axis === "y" ? value - a.bounds.y : 0;
    if (dx || dy) void run({ op: "layer.offset", ids: pro.selection(), dx, dy }, undefined, t("Move"));
  }

  // ---- Text
  async function setText(patch: Partial<TextData>) {
    const a = active;
    if (!a?.text) return;
    const data = { ...structuredClone($state.snapshot(a.text)), ...patch } as TextData;
    try {
      const { renderText } = await import("../../lib/text");
      const r = await renderText(data);
      await run({ op: "layer.set-text", id: a.id, data, width: r.width, height: r.height, x: r.x, y: r.y }, new Uint8Array(r.rgba.buffer, r.rgba.byteOffset, r.rgba.byteLength));
    } catch (e) {
      editor.error(e);
    }
  }
  let textDraft = $state<string | null>(null);
  let textColorOpen = $state(false);

  // ---- Shape
  function setShape(patch: Partial<ShapeData>, final: boolean) {
    const a = active;
    if (!a?.shape) return;
    const data = { ...structuredClone($state.snapshot(a.shape)), ...patch };
    sendRaw({ op: "layer.set-shape", id: a.id, data });
    if (final) void seal();
  }
  let shapeColor = $state<"stroke" | "fill" | null>(null);

  // ---- Fill layer
  function setFillColor(c: Rgba8, final: boolean) {
    const a = active;
    if (!a) return;
    sendRaw({ op: "layer.set-fill", id: a.id, fill: { kind: "solid", color: c } });
    if (final) void seal();
  }
</script>

<div class="panel" data-testid="properties-panel">
  {#if !editor.hasDocument || !s}
    <p class="ops-note empty">{t("Properties of the active layer show here.")}</p>
  {:else if active?.kind === "adjustment" && draft}
    {@const info = kindInfo(draft.kind)}
    <header class="head">
      {#if info}
        {@const Icon = info.icon}
        <span class="chip"><Icon size={14} /></span>
      {/if}
      <span class="title">{info?.label ?? draft.kind}</span>
    </header>
    <AdjustmentEditor value={draft} histogram={hist} onchange={onAdjust} sample={(x, y) => sampleComposite(x, y, 3, active?.id ?? null)} />
    {#if active.clip}<p class="ops-note">{t("Clipped: affects only the layer below.")}</p>{/if}
  {:else if active}
    <header class="head">
      <span class="chip"><Image size={14} /></span>
      <span class="title">
        {active.kind === "text" ? t("Type layer") : active.kind === "shape" ? t("Shape layer") : active.kind === "smart" ? t("Smart object") : active.kind === "fill" ? t("Fill layer") : active.kind === "group" ? t("Group") : t("Pixel layer")}
      </span>
    </header>

    {#if active.bounds}
      <section class="section">
        <span class="ops-section-title">{t("Transform")}</span>
        <div class="grid">
          <NumberField label="X" value={active.bounds.x} unit="px" width={70} onchange={(v) => moveTo("x", v)} />
          <NumberField label="Y" value={active.bounds.y} unit="px" width={70} onchange={(v) => moveTo("y", v)} />
          <NumberField label="W" value={active.bounds.w} unit="px" width={70} disabled />
          <NumberField label="H" value={active.bounds.h} unit="px" width={70} disabled />
        </div>
      </section>
    {/if}

    {#if active.kind === "smart" && active.smart}
      <section class="section">
        <span class="ops-section-title">{t("Smart object")}</span>
        <p class="line">{active.smart.source === "document" ? t("Embedded document, {n} layers", { n: active.smart.layers }) : t("Embedded pixels")} · {active.smart.width} × {active.smart.height} px</p>
        <p class="line">{active.smart.filters.length === 1 ? t("1 smart filter") : t("{n} smart filters", { n: active.smart.filters.length })}</p>
        <div class="ops-row">
          <button type="button" class="oa-btn oa-btn--secondary small" onclick={() => replaceSmartContents(active.id)}>{t("Replace Contents…")}</button>
          <button type="button" class="oa-btn oa-btn--secondary small" onclick={() => unpackSmart(active.id)}>{t("Convert to Layers")}</button>
        </div>
      </section>
    {/if}

    {#if active.kind === "text" && active.text}
      {@const d = active.text}
      <section class="section">
        <span class="ops-section-title">{t("Character")}</span>
        <textarea
          class="ops-field text"
          rows="3"
          aria-label={t("Text")}
          value={textDraft ?? d.text}
          oninput={(e) => (textDraft = e.currentTarget.value)}
          onblur={() => {
            if (textDraft != null && textDraft !== d.text) void setText({ text: textDraft });
            textDraft = null;
          }}
        ></textarea>
        <div class="ops-row">
          <input class="ops-field grow" aria-label={t("Font family")} value={d.font_family} onchange={(e) => setText({ font_family: e.currentTarget.value })} />
        </div>
        <div class="grid">
          <NumberField label={t("Size")} value={d.font_size} min={1} max={2000} unit="px" width={64} onchange={(v) => setText({ font_size: v })} />
          <NumberField label={t("Weight")} value={d.font_weight} min={100} max={900} step={100} width={52} onchange={(v) => setText({ font_weight: v })} />
          <NumberField label={t("Leading")} value={d.line_height} min={0.5} max={4} step={0.05} width={52} onchange={(v) => setText({ line_height: v })} />
          <NumberField label={t("Tracking")} value={d.letter_spacing} min={-50} max={200} step={0.5} width={52} onchange={(v) => setText({ letter_spacing: v })} />
        </div>
        <div class="ops-row">
          <SegmentedControl
            ariaLabel={t("Alignment")}
            size="xs"
            options={[
              { value: "left", label: t("Left") },
              { value: "center", label: t("Centre") },
              { value: "right", label: t("Right") },
            ]}
            value={d.align}
            onchange={(v) => setText({ align: v })}
          />
          <label class="ops-check"><input type="checkbox" checked={d.italic} onchange={(e) => setText({ italic: e.currentTarget.checked })} />{t("Italic")}</label>
          <span class="grow"></span>
          <ColorSwatch color={d.color} size={20} label={t("Text colour")} onclick={() => (textColorOpen = !textColorOpen)} />
        </div>
        {#if textColorOpen}
          <ColorPicker value={d.color} height={80} onchange={(c) => setText({ color: c })} />
        {/if}
      </section>
    {/if}

    {#if active.kind === "shape" && active.shape}
      {@const d = active.shape}
      <section class="section">
        <span class="ops-section-title">{t("Appearance")}</span>
        <div class="ops-row">
          <span class="ops-label">{t("Fill")}</span>
          <ColorSwatch color={d.fill} size={20} onclick={() => (shapeColor = shapeColor === "fill" ? null : "fill")} />
          <span class="ops-label">{t("Stroke")}</span>
          <ColorSwatch color={d.stroke} size={20} onclick={() => (shapeColor = shapeColor === "stroke" ? null : "stroke")} />
          <span class="grow"></span>
          <NumberField value={d.stroke_width} min={0} max={500} unit="px" width={56} ariaLabel={t("Stroke width")} onchange={(v) => setShape({ stroke_width: v }, true)} />
        </div>
        {#if shapeColor}
          <ColorPicker
            value={(shapeColor === "fill" ? d.fill : d.stroke) ?? { r: 0, g: 0, b: 0, a: 255 }}
            height={80}
            oninput={(c) => setShape({ [shapeColor!]: c }, false)}
            onchange={(c) => setShape({ [shapeColor!]: c }, true)}
          />
          <button type="button" class="oa-btn oa-btn--ghost small" onclick={() => setShape({ [shapeColor!]: null }, true)}>{shapeColor === "fill" ? t("No fill") : t("No stroke")}</button>
        {/if}
        {#if d.kind === "rect"}
          <Slider label={t("Corner radius")} value={d.corner_radius} min={0} max={500} unit="px" defaultValue={0} oninput={(v) => setShape({ corner_radius: v }, false)} onchange={(v) => setShape({ corner_radius: v }, true)} />
        {/if}
      </section>
    {/if}

    {#if active.kind === "fill" && active.fill?.kind === "solid"}
      <section class="section">
        <span class="ops-section-title">{t("Solid colour")}</span>
        <ColorPicker value={active.fill.color} height={110} oninput={(c) => setFillColor(c, false)} onchange={(c) => setFillColor(c, true)} />
      </section>
    {/if}

    {#if active.mask}
      {@const m = active.mask}
      <section class="section">
        <div class="ops-row">
          <span class="ops-section-title grow"><SquareDashed size={11} /> {t("Layer mask")}</span>
          <label class="ops-check"><input type="checkbox" checked={m.enabled} onchange={(e) => run({ op: "layer.mask-props", id: active.id, enabled: e.currentTarget.checked })} />{t("Enabled")}</label>
        </div>
        <Slider
          label={t("Density")}
          value={maskDensity ?? Math.round(m.density * 100)}
          min={0}
          max={100}
          unit="%"
          defaultValue={100}
          oninput={(v) => {
            maskDensity = v;
            sendRaw({ op: "layer.mask-props", id: active.id, density: v / 100 });
          }}
          onchange={async () => {
            await seal();
            maskDensity = null;
          }}
        />
        <div class="ops-row">
          <button type="button" class="oa-btn oa-btn--secondary small" onclick={() => ACTIONS["layer.mask-apply"].run()} disabled={active.kind !== "pixel"}>{t("Apply mask")}</button>
          <button type="button" class="oa-btn oa-btn--ghost small" onclick={() => ACTIONS["layer.mask-delete"].run()}>{t("Delete mask")}</button>
        </div>
      </section>
    {/if}
  {:else}
    <header class="head">
      <span class="chip"><Image size={14} /></span>
      <span class="title">{t("Document")}</span>
    </header>
    <section class="section">
      <div class="grid">
        <NumberField label="W" value={s.width} unit="px" width={70} disabled />
        <NumberField label="H" value={s.height} unit="px" width={70} disabled />
      </div>
      <NumberField label={t("Resolution")} value={s.resolution} unit="ppi" width={70} min={1} max={10000} onchange={(v) => run({ op: "doc.set-resolution", resolution: v })} />
      <div class="ops-row">
        <button type="button" class="oa-btn oa-btn--secondary small" onclick={() => pro.open({ kind: "image-size" })}>{t("Image Size…")}</button>
        <button type="button" class="oa-btn oa-btn--secondary small" onclick={() => pro.open({ kind: "canvas-size" })}>{t("Canvas Size…")}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 8px 10px 12px;
  }
  .empty {
    padding: var(--space-4) 0;
    text-align: center;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chip {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-xs);
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .title {
    font: var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-strong);
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 8px;
    border-top: var(--border-width) solid var(--border-hairline);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 10px;
    justify-items: end;
  }
  .line {
    margin: 0;
    font: var(--type-caption);
    color: var(--text-body);
  }
  .small {
    height: 22px;
    padding: 0 8px;
    font-size: var(--text-xs);
  }
  .text {
    height: auto;
    padding: 6px;
    resize: vertical;
    font: var(--weight-regular) var(--text-xs) / 1.4 var(--font-sans);
  }
  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
  }
</style>
