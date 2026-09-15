<script lang="ts">
  // Layer › Layer Style: effects on the left (tick to turn on, click to
  // edit), parameters on the right, the canvas previewing every change.
  // Cancel takes the preview back through history.
  import { onDestroy } from "svelte";
  import { BLEND_LABELS, BLEND_MODES, allLayers, type BlendMode, type Rgba8 } from "../../engine/types";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import ColorSwatch from "../../ui/ColorSwatch.svelte";
  import Dialog from "../../ui/Dialog.svelte";
  import GradientEditor from "../../ui/GradientEditor.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";
  import Slider from "../../ui/Slider.svelte";
  import { PreviewSession } from "../engine.svelte";
  import { pro } from "../state.svelte";
  import { EFFECTS, emptyEffects, type EffectKey, type LayerEffects, type ProLayer } from "../types";

  let { id, section = "blending" }: { id: number; section?: string } = $props();

  const layer = (() => allLayers(editor.summary?.layers ?? []).find((l) => l.id === id) as unknown as ProLayer | undefined)();
  const startFx: LayerEffects = layer?.effects ? structuredClone($state.snapshot(layer.effects)) : emptyEffects();
  const startProps = { blend: layer?.blend ?? "normal", opacity: layer?.opacity ?? 1, fillOpacity: layer?.fillOpacity ?? 1 };

  let fx = $state<LayerEffects>(structuredClone(startFx));
  let layerProps = $state({ ...startProps });
  const initialSection = () => section;
  let current = $state<string>(initialSection());
  let colorKey = $state<string | null>(null);
  let closing = false;
  const session = new PreviewSession();

  // Clicking an effect that is off turns it on, as Photoshop does.
  (() => {
    const sec = initialSection();
    if (sec !== "blending" && !fx[sec as EffectKey]) {
      const def = EFFECTS.find((e) => e.key === sec);
      if (def) (fx as unknown as Record<string, unknown>)[sec] = def.make();
    }
  })();

  function apply() {
    return (async () => {
      const f = $state.snapshot(fx);
      const hasAny = EFFECTS.some((e) => f[e.key]);
      await session.exec({ op: "layer.set-effects", id, effects: hasAny ? f : null });
      const p = $state.snapshot(layerProps);
      if (p.blend !== startProps.blend || p.opacity !== startProps.opacity || p.fillOpacity !== startProps.fillOpacity) {
        await session.exec({ op: "layer.props", id, blend: p.blend, opacity: p.opacity, fill_opacity: p.fillOpacity });
      }
    })();
  }
  const key = () => JSON.stringify([fx, layerProps]);
  let timer = 0;
  $effect(() => {
    const k = key();
    clearTimeout(timer);
    if (closing) return;
    timer = window.setTimeout(() => session.request(k, apply), 50);
  });
  onDestroy(() => clearTimeout(timer));

  async function ok() {
    closing = true;
    clearTimeout(timer);
    if (await session.commit(key(), apply)) {
      await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
      pro.close();
    } else closing = false;
  }
  async function cancel() {
    closing = true;
    clearTimeout(timer);
    await session.cancel();
    pro.close();
  }

  function toggle(k: EffectKey, on: boolean) {
    const cur = fx[k];
    if (on && !cur) (fx as unknown as Record<string, unknown>)[k] = EFFECTS.find((e) => e.key === k)!.make();
    else if (cur) (cur as { enabled: boolean }).enabled = on;
    current = k;
  }

  const blendOptions = BLEND_MODES.map((m) => (m ? { value: m, label: t(BLEND_LABELS[m]) } : null));
  // A typed view of the effect being edited.
  const e = $derived((current !== "blending" ? fx[current as EffectKey] : null) as Record<string, any> | null);
  const set = (k: string, v: unknown) => {
    if (e) e[k] = v;
  };
</script>

<Dialog title={t("Layer Style")} width={620} scrim="clear" testid="layer-style-dialog" onclose={cancel} onsubmit={ok}>
  <div class="layout">
    <nav class="list" aria-label={t("Effects")}>
      <button type="button" class="item head" class:on={current === "blending"} onclick={() => (current = "blending")}>{t("Blending Options")}</button>
      <span class="ops-section-title cap">{t("Styles")}</span>
      {#each EFFECTS as def (def.key)}
        {@const val = fx[def.key]}
        <div class="item" class:on={current === def.key}>
          <input type="checkbox" class="check" aria-label={t("Turn on {name}", { name: t(def.label) })} checked={!!val && val.enabled} onchange={(ev) => toggle(def.key, ev.currentTarget.checked)} />
          <button type="button" class="name" data-testid="style-{def.key}" onclick={() => toggle(def.key, true)}>{t(def.label)}</button>
        </div>
      {/each}
    </nav>

    <div class="params ops-stack">
      {#if current === "blending"}
        <span class="ops-section-title">{t("General blending")}</span>
        <div class="ops-row">
          <span class="ops-label lbl">{t("Blend mode")}</span>
          <Select ariaLabel={t("Blend mode")} value={layerProps.blend} width={160} options={blendOptions} onchange={(v) => (layerProps.blend = v as BlendMode)} />
        </div>
        <Slider label={t("Opacity")} value={Math.round(layerProps.opacity * 100)} min={0} max={100} unit="%" defaultValue={100} oninput={(v) => (layerProps.opacity = v / 100)} />
        <span class="ops-section-title">{t("Advanced blending")}</span>
        <Slider label={t("Fill opacity")} value={Math.round(layerProps.fillOpacity * 100)} min={0} max={100} unit="%" defaultValue={100} oninput={(v) => (layerProps.fillOpacity = v / 100)} />
        <Slider label={t("Scale effects")} value={Math.round(fx.scale * 100)} min={1} max={1000} unit="%" defaultValue={100} oninput={(v) => (fx.scale = v / 100)} />
        <label class="ops-check"><input type="checkbox" bind:checked={fx.enabled} />{t("Show effects")}</label>
      {:else if e}
        <span class="ops-section-title">{t(EFFECTS.find((d) => d.key === current)!.label)}</span>
        {#if current === "bevel"}
          <div class="ops-row">
            <span class="ops-label lbl">{t("Style")}</span>
            <Select
              ariaLabel={t("Style")}
              value={e.style}
              width={140}
              options={[
                { value: "inner-bevel", label: t("Inner Bevel") },
                { value: "outer-bevel", label: t("Outer Bevel") },
                { value: "emboss", label: t("Emboss") },
                { value: "pillow-emboss", label: t("Pillow Emboss") },
              ]}
              onchange={(v) => set("style", v)}
            />
          </div>
          <div class="ops-row">
            <span class="ops-label lbl">{t("Technique")}</span>
            <SegmentedControl ariaLabel={t("Technique")} size="xs" options={[{ value: "smooth", label: t("Smooth") }, { value: "chisel-hard", label: t("Chisel hard") }]} value={e.technique} onchange={(v) => set("technique", v)} />
          </div>
          <Slider label={t("Depth")} value={e.depth} min={1} max={1000} unit="%" defaultValue={100} oninput={(v) => set("depth", v)} />
          <div class="ops-row">
            <span class="ops-label lbl">{t("Direction")}</span>
            <SegmentedControl ariaLabel={t("Direction")} size="xs" options={[{ value: "up", label: t("Up") }, { value: "down", label: t("Down") }]} value={e.down ? "down" : "up"} onchange={(v) => set("down", v === "down")} />
          </div>
          <Slider label={t("Size")} value={e.size} min={0} max={250} unit="px" defaultValue={5} oninput={(v) => set("size", v)} />
          <Slider label={t("Soften")} value={e.soften} min={0} max={16} unit="px" defaultValue={0} oninput={(v) => set("soften", v)} />
          <Slider label={t("Angle")} value={e.angle} min={-180} max={180} unit="°" defaultValue={120} oninput={(v) => set("angle", v)} />
          <Slider label={t("Altitude")} value={e.altitude} min={0} max={90} unit="°" defaultValue={30} oninput={(v) => set("altitude", v)} />
          {#each [["highlight", t("Highlight")], ["shadow", t("Shadow")]] as [p, label] (p)}
            <div class="ops-row">
              <span class="ops-label lbl">{label}</span>
              <Select ariaLabel={t("{what} mode", { what: label })} value={e[`${p}_blend`]} width={120} options={blendOptions} onchange={(v) => set(`${p}_blend`, v)} />
              <ColorSwatch color={e[`${p}_color`]} size={20} onclick={() => (colorKey = colorKey === `${p}_color` ? null : `${p}_color`)} />
            </div>
            <Slider label={t("{what} opacity", { what: label })} value={Math.round(e[`${p}_opacity`] * 100)} min={0} max={100} unit="%" oninput={(v) => set(`${p}_opacity`, v / 100)} />
          {/each}
        {:else}
          {#if "blend" in e}
            <div class="ops-row">
              <span class="ops-label lbl">{t("Blend mode")}</span>
              <Select ariaLabel={t("Blend mode")} value={e.blend} width={140} options={blendOptions} onchange={(v) => set("blend", v)} />
              {#if "color" in e}<ColorSwatch color={e.color} size={20} label={t("Effect colour")} onclick={() => (colorKey = colorKey === "color" ? null : "color")} />{/if}
            </div>
          {/if}
          {#if "opacity" in e}
            <Slider label={t("Opacity")} value={Math.round(e.opacity * 100)} min={0} max={100} unit="%" defaultValue={100} testid="style-opacity" oninput={(v) => set("opacity", v / 100)} />
          {/if}
          {#if current === "gradient_overlay"}
            <GradientEditor stops={e.stops} onchange={(stops) => set("stops", stops)} />
            <div class="ops-row">
              <span class="ops-label lbl">{t("Style")}</span>
              <Select
                ariaLabel={t("Style")}
                value={e.gradient}
                width={120}
                options={["linear", "radial", "angle", "reflected", "diamond"].map((g) => ({ value: g, label: t(g.charAt(0).toUpperCase() + g.slice(1)) }))}
                onchange={(v) => set("gradient", v)}
              />
              <label class="ops-check"><input type="checkbox" checked={e.reverse} onchange={(ev) => set("reverse", ev.currentTarget.checked)} />{t("Reverse")}</label>
            </div>
            <Slider label={t("Angle")} value={e.angle} min={-180} max={180} unit="°" defaultValue={90} oninput={(v) => set("angle", v)} />
            <Slider label={t("Scale")} value={e.scale} min={10} max={150} unit="%" defaultValue={100} oninput={(v) => set("scale", v)} />
          {/if}
          {#if current === "stroke"}
            <Slider label={t("Size")} value={e.size} min={1} max={250} unit="px" defaultValue={3} oninput={(v) => set("size", v)} />
            <div class="ops-row">
              <span class="ops-label lbl">{t("Position")}</span>
              <SegmentedControl ariaLabel={t("Position")} size="xs" options={[{ value: "outside", label: t("Outside") }, { value: "inside", label: t("Inside") }, { value: "center", label: t("Centre") }]} value={e.position} onchange={(v) => set("position", v)} />
            </div>
          {/if}
          {#if "angle" in e && current !== "gradient_overlay"}
            <Slider label={t("Angle")} value={e.angle} min={-180} max={180} unit="°" oninput={(v) => set("angle", v)} />
          {/if}
          {#if "distance" in e}
            <Slider label={t("Distance")} value={e.distance} min={0} max={1000} unit="px" oninput={(v) => set("distance", v)} />
          {/if}
          {#if "spread" in e}
            <Slider label={current.startsWith("inner") ? t("Choke") : t("Spread")} value={e.spread} min={0} max={100} unit="%" defaultValue={0} oninput={(v) => set("spread", v)} />
          {/if}
          {#if "size" in e && current !== "stroke"}
            <Slider label={t("Size")} value={e.size} min={0} max={250} unit="px" defaultValue={5} testid="style-size" oninput={(v) => set("size", v)} />
          {/if}
          {#if current === "inner_glow"}
            <div class="ops-row">
              <span class="ops-label lbl">{t("Source")}</span>
              <SegmentedControl ariaLabel={t("Source")} size="xs" options={[{ value: "center", label: t("Centre") }, { value: "edge", label: t("Edge") }]} value={e.source} onchange={(v) => set("source", v)} />
            </div>
          {/if}
          {#if current === "satin"}
            <label class="ops-check"><input type="checkbox" checked={e.invert} onchange={(ev) => set("invert", ev.currentTarget.checked)} />{t("Invert")}</label>
          {/if}
        {/if}
        {#if colorKey && e[colorKey]}
          <ColorPicker value={e[colorKey] as Rgba8} height={90} oninput={(c) => set(colorKey!, c)} />
        {/if}
        <div class="ops-row">
          <button
            type="button"
            class="oa-btn oa-btn--ghost small"
            onclick={() => {
              const def = EFFECTS.find((d) => d.key === current)!;
              (fx as unknown as Record<string, unknown>)[current] = def.make();
            }}>{t("Reset to default")}</button
          >
          <button
            type="button"
            class="oa-btn oa-btn--ghost small"
            onclick={() => {
              (fx as unknown as Record<string, unknown>)[current] = null;
              current = "blending";
            }}>{t("Remove effect")}</button
          >
        </div>
      {/if}
      {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
    </div>
  </div>
  {#snippet footer()}
    <span class="grow ops-note">{layer?.name ?? ""}</span>
    <button type="button" class="oa-btn oa-btn--secondary" onclick={cancel}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={ok}>{t("OK")}</button>
  {/snippet}
</Dialog>

<style>
  .layout {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 16px;
    min-height: 380px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-right: 12px;
    border-right: 1px solid var(--border-hairline);
  }
  .cap {
    padding: 10px 6px 4px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 6px;
    border-radius: var(--radius-xs);
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-body);
  }
  button.item,
  .name {
    border: 0;
    background: transparent;
    text-align: left;
    color: inherit;
    font: inherit;
    cursor: default;
  }
  .name {
    flex: 1;
    height: 100%;
    padding: 0;
  }
  .item:hover {
    background: var(--surface-hover);
  }
  .item.on {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .check {
    accent-color: var(--text-strong);
    width: 13px;
    height: 13px;
    margin: 0;
  }
  .params {
    max-height: 480px;
    overflow-y: auto;
    padding-right: 4px;
    gap: 8px;
  }
  .lbl {
    width: 80px;
  }
  .small {
    height: 22px;
    padding: 0 8px;
    font-size: var(--text-xs);
  }
  .grow {
    flex: 1;
  }
</style>
