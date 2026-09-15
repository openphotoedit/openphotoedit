<script lang="ts">
  // Photoshop's picker in miniature: a saturation/brightness field, a hue
  // strip, HSB and RGB fields and a hex field. Hue is kept locally so it
  // survives dragging through grey.
  import type { Rgba8 } from "../engine/types";
  import { t } from "../lib/i18n";
  import NumberField from "./NumberField.svelte";
  import { fromHex, hsbToRgb, rgbToHsb, sameColor, toHex, type Hsb } from "./color";

  let {
    value,
    height = 150,
    showFields = true,
    testid,
    oninput,
    onchange,
  }: {
    value: Rgba8;
    height?: number;
    showFields?: boolean;
    testid?: string;
    oninput?: (c: Rgba8) => void;
    onchange?: (c: Rgba8) => void;
  } = $props();

  const initial = () => value;
  let hsb = $state<Hsb>(rgbToHsb(initial()));
  let last: Rgba8 = { ...initial() };

  // Follow outside changes without losing hue on greys.
  $effect(() => {
    const v = value;
    if (!sameColor(v, last)) {
      const next = rgbToHsb(v);
      if (next.s === 0 || next.b === 0) next.h = hsb.h;
      if (next.b === 0) next.s = hsb.s;
      hsb = next;
      last = { ...v };
    }
  });

  function emit(next: Hsb, final: boolean) {
    hsb = next;
    const c = hsbToRgb(next, value.a ?? 255);
    last = c;
    oninput?.(c);
    if (final) onchange?.(c);
  }

  function emitRgb(c: Rgba8, final = true) {
    const next = rgbToHsb(c);
    if (next.s === 0) next.h = hsb.h;
    hsb = next;
    last = c;
    oninput?.(c);
    if (final) onchange?.(c);
  }

  let field: HTMLDivElement;
  let strip: HTMLDivElement;
  let drag: "field" | "strip" | null = null;

  function pointer(e: PointerEvent, final = false) {
    if (drag === "field") {
      const r = field.getBoundingClientRect();
      const s = Math.min(100, Math.max(0, ((e.clientX - r.left) / r.width) * 100));
      const b = Math.min(100, Math.max(0, (1 - (e.clientY - r.top) / r.height) * 100));
      emit({ h: hsb.h, s, b }, final);
    } else if (drag === "strip") {
      const r = strip.getBoundingClientRect();
      const h = Math.min(359.9, Math.max(0, (1 - (e.clientY - r.top) / r.height) * 360));
      emit({ ...hsb, h }, final);
    }
  }

  function down(kind: "field" | "strip", e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = kind;
    pointer(e);
  }
  function up(e: PointerEvent) {
    if (!drag) return;
    pointer(e, true);
    drag = null;
  }

  function keyField(e: KeyboardEvent) {
    const d = e.shiftKey ? 10 : 1;
    const map: Record<string, [number, number]> = { ArrowLeft: [-d, 0], ArrowRight: [d, 0], ArrowUp: [0, d], ArrowDown: [0, -d] };
    const m = map[e.key];
    if (!m) return;
    e.preventDefault();
    emit({ h: hsb.h, s: Math.min(100, Math.max(0, hsb.s + m[0])), b: Math.min(100, Math.max(0, hsb.b + m[1])) }, true);
  }
  function keyStrip(e: KeyboardEvent) {
    const d = (e.shiftKey ? 10 : 1) * (e.key === "ArrowUp" ? 1 : e.key === "ArrowDown" ? -1 : 0);
    if (!d) return;
    e.preventDefault();
    emit({ ...hsb, h: (hsb.h + d + 360) % 360 }, true);
  }

  const rgb = $derived(hsbToRgb(hsb, value.a ?? 255));
  const hueCss = $derived(`hsl(${hsb.h} 100% 50%)`);
</script>

<div class="ops-picker" data-testid={testid}>
  <div class="ops-picker__maps" style:height="{height}px">
    <div
      class="field"
      bind:this={field}
      style:background-color={hueCss}
      role="slider"
      tabindex="0"
      aria-label={t("Saturation and brightness")}
      aria-valuetext="{Math.round(hsb.s)}%, {Math.round(hsb.b)}%"
      aria-valuenow={Math.round(hsb.s)}
      onpointerdown={(e) => down("field", e)}
      onpointermove={(e) => drag && pointer(e)}
      onpointerup={up}
      onpointercancel={up}
      onkeydown={keyField}
    >
      <span class="knob" style:left="{hsb.s}%" style:top="{100 - hsb.b}%"></span>
    </div>
    <div
      class="strip"
      bind:this={strip}
      role="slider"
      tabindex="0"
      aria-label={t("Hue")}
      aria-valuemin={0}
      aria-valuemax={360}
      aria-valuenow={Math.round(hsb.h)}
      onpointerdown={(e) => down("strip", e)}
      onpointermove={(e) => drag && pointer(e)}
      onpointerup={up}
      onpointercancel={up}
      onkeydown={keyStrip}
    >
      <span class="notch" style:top="{100 - (hsb.h / 360) * 100}%"></span>
    </div>
  </div>
  {#if showFields}
    <div class="ops-picker__fields">
      <NumberField label="H" unit="°" width={50} min={0} max={360} value={Math.round(hsb.h)} onchange={(v) => emit({ ...hsb, h: v % 360 }, true)} />
      <NumberField label="S" unit="%" width={50} min={0} max={100} value={Math.round(hsb.s)} onchange={(v) => emit({ ...hsb, s: v }, true)} />
      <NumberField label="B" unit="%" width={50} min={0} max={100} value={Math.round(hsb.b)} onchange={(v) => emit({ ...hsb, b: v }, true)} />
      <NumberField label="R" width={50} min={0} max={255} value={rgb.r} onchange={(v) => emitRgb({ ...rgb, r: v })} />
      <NumberField label="G" width={50} min={0} max={255} value={rgb.g} onchange={(v) => emitRgb({ ...rgb, g: v })} />
      <NumberField label="B" width={50} min={0} max={255} value={rgb.b} onchange={(v) => emitRgb({ ...rgb, b: v })} />
      <label class="hex">
        <span class="ops-label">#</span>
        <input
          class="ops-field"
          spellcheck="false"
          aria-label={t("Hex colour")}
          value={toHex(rgb)}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              const c = fromHex(e.currentTarget.value);
              if (c) emitRgb({ ...c, a: value.a ?? 255 });
              e.preventDefault();
              e.stopPropagation();
            }
          }}
          onchange={(e) => {
            const c = fromHex(e.currentTarget.value);
            if (c) emitRgb({ ...c, a: value.a ?? 255 });
          }}
        />
      </label>
    </div>
  {/if}
</div>

<style>
  .ops-picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .ops-picker__maps {
    display: flex;
    gap: var(--space-2);
  }
  .field {
    position: relative;
    flex: 1;
    border-radius: var(--radius-xs);
    background-image: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent);
    cursor: crosshair;
    touch-action: none;
    box-shadow: inset 0 0 0 1px var(--border-hairline);
  }
  .strip {
    position: relative;
    width: 14px;
    border-radius: var(--radius-xs);
    background: linear-gradient(to top, #f00 0%, #ff0 16.66%, #0f0 33.33%, #0ff 50%, #00f 66.66%, #f0f 83.33%, #f00 100%);
    cursor: ns-resize;
    touch-action: none;
  }
  .field:focus-visible,
  .strip:focus-visible {
    box-shadow: 0 0 0 2px var(--border-focus);
  }
  .knob {
    position: absolute;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1.5px solid #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.5);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }
  .notch {
    position: absolute;
    left: -3px;
    right: -3px;
    height: 4px;
    border-radius: 2px;
    border: 1.5px solid #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.5);
    transform: translateY(-50%);
    pointer-events: none;
  }
  .ops-picker__fields {
    display: grid;
    grid-template-columns: repeat(3, auto);
    justify-content: space-between;
    gap: 4px var(--space-2);
  }
  .hex {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hex input {
    flex: 1;
    font-family: var(--font-mono);
    text-transform: lowercase;
  }
</style>
