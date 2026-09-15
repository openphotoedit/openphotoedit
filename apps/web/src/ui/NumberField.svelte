<script lang="ts">
  // A numeric field with Photoshop's scrubby label: drag the label left or
  // right to change the value (Shift ×10, Alt ×0.1). Arrow keys step, Enter
  // commits. `oninput` fires continuously, `onchange` once at the end.
  import "./primitives.css";
  import { t } from "../lib/i18n";
  import { evalArithmetic } from "./arith";

  let {
    value,
    min = -Infinity,
    max = Infinity,
    step = 1,
    precision,
    unit = "",
    label,
    ariaLabel,
    width = 52,
    disabled = false,
    testid,
    oninput,
    onchange,
  }: {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    precision?: number;
    unit?: string;
    label?: string;
    ariaLabel?: string;
    width?: number;
    disabled?: boolean;
    testid?: string;
    oninput?: (v: number) => void;
    onchange?: (v: number) => void;
  } = $props();

  const digits = $derived(precision ?? (step >= 1 ? 0 : Math.min(4, Math.ceil(-Math.log10(step)))));
  let focused = $state(false);
  let text = $state("");
  let scrubbing = $state(false);

  const shown = $derived(Number.isFinite(value) ? value.toFixed(digits).replace(/^-0(\.0+)?$/, "0") : "");

  function clamp(v: number) {
    const k = Math.pow(10, digits);
    return Math.min(max, Math.max(min, Math.round(v * k) / k));
  }

  function commit(v: number, final = true) {
    const c = clamp(v);
    if (!Number.isFinite(c)) return;
    oninput?.(c);
    if (final) onchange?.(c);
  }

  function parseText(s: string): number {
    const cleaned = (unit ? s.replace(unit, "") : s).replace(/,/g, ".").trim();
    const v = evalArithmetic(cleaned);
    return Number.isFinite(v) ? v : parseFloat(cleaned);
  }

  function onKey(e: KeyboardEvent) {
    const input = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") {
      const v = parseText(text);
      if (Number.isFinite(v)) commit(v);
      text = clamp(Number.isFinite(v) ? v : value).toFixed(digits);
      input.select();
      e.preventDefault();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const base = Number.isFinite(parseText(text)) ? parseText(text) : value;
      const v = clamp(base + (e.key === "ArrowUp" ? 1 : -1) * step * (e.shiftKey ? 10 : 1));
      text = v.toFixed(digits);
      commit(v);
    } else if (e.key === "Escape") {
      text = shown;
      input.blur();
    }
  }

  let scrub: { x: number; start: number; moved: boolean } | null = null;

  function scrubDown(e: PointerEvent) {
    if (disabled || e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    scrub = { x: e.clientX, start: value, moved: false };
  }
  function scrubMove(e: PointerEvent) {
    if (!scrub) return;
    const dx = e.clientX - scrub.x;
    if (!scrub.moved && Math.abs(dx) < 2) return;
    scrub.moved = true;
    scrubbing = true;
    const k = e.shiftKey ? 10 : e.altKey ? 0.1 : 1;
    const v = clamp(scrub.start + Math.round(dx / 2) * step * k);
    if (v !== value) oninput?.(v);
  }
  function scrubUp() {
    if (!scrub) return;
    const moved = scrub.moved;
    scrub = null;
    scrubbing = false;
    if (moved) onchange?.(value);
  }
</script>

<span class="ops-number" class:disabled data-testid={testid}>
  {#if label}
    <span
      class="ops-number__label"
      class:scrubbing
      role="presentation"
      onpointerdown={scrubDown}
      onpointermove={scrubMove}
      onpointerup={scrubUp}
      onpointercancel={scrubUp}>{label}</span
    >
  {/if}
  <span class="ops-number__box" style:width="{width}px">
    <input
      class="ops-field ops-number__input"
      type="text"
      inputmode="decimal"
      spellcheck="false"
      aria-label={ariaLabel ?? label ?? t("Value")}
      {disabled}
      value={focused ? text : shown}
      onfocus={(e) => {
        focused = true;
        text = shown;
        queueMicrotask(() => (e.currentTarget as HTMLInputElement | null)?.select());
      }}
      onblur={() => {
        focused = false;
        const v = parseText(text);
        if (Number.isFinite(v) && clamp(v) !== value) commit(v);
      }}
      oninput={(e) => (text = e.currentTarget.value)}
      onkeydown={onKey}
    />
    {#if unit}<span class="ops-number__unit" aria-hidden="true">{unit}</span>{/if}
  </span>
</span>

<style>
  .ops-number {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .ops-number.disabled {
    opacity: 0.5;
  }
  .ops-number__label {
    font: var(--type-caption);
    color: var(--text-muted);
    cursor: ew-resize;
    user-select: none;
    white-space: nowrap;
    touch-action: none;
  }
  .ops-number__label:hover,
  .ops-number__label.scrubbing {
    color: var(--text-strong);
  }
  .ops-number__box {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
  }
  .ops-number__input {
    width: 100%;
    text-align: right;
  }
  .ops-number__box:has(.ops-number__unit) .ops-number__input {
    padding-right: 18px;
  }
  .ops-number__unit {
    position: absolute;
    right: 5px;
    top: 50%;
    transform: translateY(-50%);
    font: var(--type-caption);
    font-size: var(--text-2xs);
    color: var(--text-faint);
    pointer-events: none;
  }
</style>
