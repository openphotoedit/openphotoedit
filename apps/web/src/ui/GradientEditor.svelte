<script lang="ts">
  // A gradient bar with colour stops: click the bar to add a stop, drag a
  // stop to move it, drag it away (or press Delete) to remove it, pick its
  // colour below.
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import type { GradientStop, Rgba8 } from "../engine/types";
  import { t } from "../lib/i18n";
  import ColorPicker from "./ColorPicker.svelte";
  import IconButton from "./IconButton.svelte";
  import NumberField from "./NumberField.svelte";
  import { css } from "./color";

  let { stops, testid, onchange }: { stops: GradientStop[]; testid?: string; onchange: (stops: GradientStop[], final: boolean) => void } = $props();

  let bar: HTMLDivElement;
  let sel = $state(0);
  let drag: { i: number; out: boolean } | null = null;

  const sorted = $derived([...stops].sort((a, b) => a.pos - b.pos));
  const cssGradient = $derived(`linear-gradient(to right, ${sorted.map((s) => `${css(s.color)} ${(s.pos * 100).toFixed(2)}%`).join(", ")})`);

  function colorAt(pos: number): Rgba8 {
    const s = sorted;
    if (pos <= s[0].pos) return { ...s[0].color };
    for (let i = 0; i < s.length - 1; i++) {
      if (pos <= s[i + 1].pos) {
        const k = (pos - s[i].pos) / Math.max(1e-6, s[i + 1].pos - s[i].pos);
        const a = s[i].color;
        const b = s[i + 1].color;
        return { r: Math.round(a.r + (b.r - a.r) * k), g: Math.round(a.g + (b.g - a.g) * k), b: Math.round(a.b + (b.b - a.b) * k), a: 255 };
      }
    }
    return { ...s[s.length - 1].color };
  }

  function posOf(e: PointerEvent) {
    const r = bar.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  function barDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const pos = posOf(e);
    const next = [...stops, { pos, color: colorAt(pos) }];
    sel = next.length - 1;
    onchange(next, true);
  }

  function stopDown(e: PointerEvent, i: number) {
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    sel = i;
    drag = { i, out: false };
  }
  function stopMove(e: PointerEvent) {
    if (!drag) return;
    const r = bar.getBoundingClientRect();
    drag.out = stops.length > 2 && e.clientY > r.bottom + 28;
    const next = stops.map((s, j) => (j === drag!.i ? { ...s, pos: posOf(e) } : s));
    onchange(next, false);
  }
  function stopUp() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.out) remove(d.i);
    else onchange(stops, true);
  }
  function remove(i: number) {
    if (stops.length <= 2) return;
    const next = stops.filter((_, j) => j !== i);
    sel = 0;
    onchange(next, true);
  }
</script>

<div class="ops-gradient" data-testid={testid}>
  <div class="bar-wrap">
    <div class="bar" bind:this={bar} style:background-image={cssGradient} role="presentation" onpointerdown={barDown}></div>
    {#each stops as s, i (i)}
      <button
        type="button"
        class="stop"
        class:selected={i === sel}
        style:left="{s.pos * 100}%"
        aria-label={t("Colour stop at {pos}%", { pos: Math.round(s.pos * 100) })}
        onpointerdown={(e) => stopDown(e, i)}
        onpointermove={stopMove}
        onpointerup={stopUp}
        onkeydown={(e) => {
          if (e.key === "Delete" || e.key === "Backspace") {
            e.preventDefault();
            e.stopPropagation();
            remove(i);
          } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
            e.preventDefault();
            const d = (e.key === "ArrowRight" ? 1 : -1) * (e.shiftKey ? 0.1 : 0.01);
            onchange(stops.map((x, j) => (j === i ? { ...x, pos: Math.min(1, Math.max(0, x.pos + d)) } : x)), true);
          }
        }}
      >
        <span style:background={css(s.color)}></span>
      </button>
    {/each}
  </div>
  {#if stops[sel]}
    <div class="stop-row">
      <NumberField
        label={t("Location")}
        value={Math.round(stops[sel].pos * 100)}
        min={0}
        max={100}
        unit="%"
        width={54}
        onchange={(v) => onchange(stops.map((x, j) => (j === sel ? { ...x, pos: v / 100 } : x)), true)}
      />
      <span class="grow"></span>
      <IconButton size="xs" label={t("Delete stop")} disabled={stops.length <= 2} onclick={() => remove(sel)}><Trash2 size={12} /></IconButton>
    </div>
    <ColorPicker
      value={stops[sel].color}
      height={80}
      oninput={(c) => onchange(stops.map((x, j) => (j === sel ? { ...x, color: c } : x)), false)}
      onchange={(c) => onchange(stops.map((x, j) => (j === sel ? { ...x, color: c } : x)), true)}
    />
  {/if}
</div>

<style>
  .ops-gradient {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bar-wrap {
    position: relative;
    padding: 0 6px 14px;
  }
  .bar {
    height: 22px;
    border-radius: var(--radius-xs);
    box-shadow: inset 0 0 0 1px var(--border-hairline);
    cursor: copy;
  }
  .stop {
    position: absolute;
    bottom: 0;
    width: 12px;
    height: 14px;
    margin-left: 6px;
    padding: 2px;
    transform: translateX(-50%);
    border: 1px solid var(--border-strong);
    border-radius: 2px;
    background: var(--surface-raised);
    cursor: ew-resize;
    touch-action: none;
  }
  .stop span {
    display: block;
    width: 100%;
    height: 100%;
  }
  .stop.selected {
    border-color: var(--text-strong);
  }
  .stop-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
</style>
