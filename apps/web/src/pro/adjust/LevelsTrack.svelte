<script lang="ts">
  // Levels' triangle handles under a gradient: two (output) or three (input,
  // with the gamma midpoint). Values are 0..255; `mid` is the midpoint's
  // position between the ends, 0..1.
  import { t } from "../../lib/i18n";

  let {
    lo,
    hi,
    mid = null,
    label,
    testid,
    onchange,
  }: {
    lo: number;
    hi: number;
    mid?: number | null;
    label: string;
    testid?: string;
    onchange: (v: { lo: number; hi: number; mid: number | null }, final: boolean) => void;
  } = $props();

  let track: HTMLDivElement;
  let drag: "lo" | "hi" | "mid" | null = null;

  function val(e: PointerEvent) {
    const r = track.getBoundingClientRect();
    return Math.min(255, Math.max(0, Math.round(((e.clientX - r.left) / r.width) * 255)));
  }

  function down(e: PointerEvent, which: "lo" | "hi" | "mid") {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = which;
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const v = val(e);
    if (drag === "lo") onchange({ lo: Math.min(v, hi - 2), hi, mid }, false);
    else if (drag === "hi") onchange({ lo, hi: Math.max(v, lo + 2), mid }, false);
    else onchange({ lo, hi, mid: Math.min(0.99, Math.max(0.01, (v - lo) / Math.max(1, hi - lo))) }, false);
  }
  function up() {
    if (!drag) return;
    drag = null;
    onchange({ lo, hi, mid }, true);
  }
  function key(e: KeyboardEvent, which: "lo" | "hi" | "mid") {
    const d = (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0) * (e.shiftKey ? 10 : 1);
    if (!d) return;
    e.preventDefault();
    e.stopPropagation();
    if (which === "lo") onchange({ lo: Math.min(hi - 2, Math.max(0, lo + d)), hi, mid }, true);
    else if (which === "hi") onchange({ lo, hi: Math.max(lo + 2, Math.min(255, hi + d)), mid }, true);
    else onchange({ lo, hi, mid: Math.min(0.99, Math.max(0.01, (mid ?? 0.5) + d / 255)) }, true);
  }
  const midPos = $derived(mid == null ? 0 : lo + (hi - lo) * mid);
</script>

<div class="levels-track" data-testid={testid} role="group" aria-label={label}>
  <div class="grad" bind:this={track}></div>
  <button type="button" class="tri dark" style:left="{(lo / 255) * 100}%" aria-label={t("{what} black point: {v}", { what: label, v: lo })} onpointerdown={(e) => down(e, "lo")} onpointermove={move} onpointerup={up} onkeydown={(e) => key(e, "lo")}></button>
  {#if mid != null}
    <button type="button" class="tri grey" style:left="{(midPos / 255) * 100}%" aria-label={t("{what} midtones", { what: label })} data-testid={testid ? `${testid}-mid` : undefined} onpointerdown={(e) => down(e, "mid")} onpointermove={move} onpointerup={up} onkeydown={(e) => key(e, "mid")}></button>
  {/if}
  <button type="button" class="tri light" style:left="{(hi / 255) * 100}%" aria-label={t("{what} white point: {v}", { what: label, v: hi })} onpointerdown={(e) => down(e, "hi")} onpointermove={move} onpointerup={up} onkeydown={(e) => key(e, "hi")}></button>
</div>

<style>
  .levels-track {
    position: relative;
    height: 20px;
    margin: 0 6px;
  }
  .grad {
    height: 6px;
    border-radius: 1px;
    background: linear-gradient(to right, #000, #fff);
  }
  .tri {
    position: absolute;
    top: 7px;
    width: 12px;
    height: 11px;
    padding: 0;
    border: 0;
    transform: translateX(-50%);
    clip-path: polygon(50% 0, 100% 100%, 0 100%);
    cursor: ew-resize;
    touch-action: none;
  }
  .tri::after {
    content: "";
    position: absolute;
    inset: 2px 2.5px 1px;
    clip-path: polygon(50% 0, 100% 100%, 0 100%);
  }
  .tri {
    background: var(--text-muted);
  }
  .dark::after {
    background: #000;
  }
  .grey::after {
    background: #808080;
  }
  .light::after {
    background: #fff;
  }
  .tri:focus-visible {
    background: var(--border-focus);
    box-shadow: none;
  }
</style>
