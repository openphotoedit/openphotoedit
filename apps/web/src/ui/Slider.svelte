<script lang="ts">
  // Label and value on one line, the track below: the shape of Photoshop's
  // Properties sliders. `oninput` fires while dragging, `onchange` once when
  // the drag (or key press, or typed value) ends. Double-click resets.
  import "./primitives.css";
  import NumberField from "./NumberField.svelte";

  let {
    label,
    value,
    min = 0,
    max = 100,
    step = 1,
    precision,
    defaultValue,
    unit = "",
    track,
    origin,
    disabled = false,
    testid,
    fieldWidth = 48,
    oninput,
    onchange,
  }: {
    label: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    precision?: number;
    defaultValue?: number;
    unit?: string;
    /** CSS background for the track (a gradient for colour sliders). */
    track?: string;
    /** Where the fill starts; defaults to 0 when the range spans it. */
    origin?: number;
    disabled?: boolean;
    testid?: string;
    fieldWidth?: number;
    oninput?: (v: number) => void;
    onchange?: (v: number) => void;
  } = $props();

  let trackEl: HTMLDivElement;
  let dragging = $state(false);

  const frac = (v: number) => (max === min ? 0 : (Math.min(max, Math.max(min, v)) - min) / (max - min));
  const o = $derived(origin ?? (min < 0 && max > 0 ? 0 : min));
  const pos = $derived(frac(value));
  const opos = $derived(frac(o));

  function snap(v: number) {
    const s = Math.round((v - min) / step) * step + min;
    const k = Math.pow(10, precision ?? (step >= 1 ? 0 : Math.ceil(-Math.log10(step))));
    return Math.min(max, Math.max(min, Math.round(s * k) / k));
  }

  function fromPointer(e: PointerEvent) {
    const r = trackEl.getBoundingClientRect();
    const f = Math.min(1, Math.max(0, (e.clientX - r.left) / Math.max(1, r.width)));
    return snap(min + f * (max - min));
  }

  function down(e: PointerEvent) {
    if (disabled || e.button !== 0) return;
    e.preventDefault();
    trackEl.setPointerCapture(e.pointerId);
    trackEl.focus();
    dragging = true;
    const v = fromPointer(e);
    if (v !== value) oninput?.(v);
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    const v = fromPointer(e);
    if (v !== value) oninput?.(v);
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    onchange?.(value);
  }
  function key(e: KeyboardEvent) {
    if (disabled) return;
    const big = (max - min) / 10;
    let v: number | null = null;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") v = value + step * (e.shiftKey ? 10 : 1);
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") v = value - step * (e.shiftKey ? 10 : 1);
    else if (e.key === "PageUp") v = value + big;
    else if (e.key === "PageDown") v = value - big;
    else if (e.key === "Home") v = min;
    else if (e.key === "End") v = max;
    if (v == null) return;
    e.preventDefault();
    e.stopPropagation();
    v = snap(v);
    oninput?.(v);
    onchange?.(v);
  }
  function reset() {
    if (disabled || defaultValue == null) return;
    oninput?.(defaultValue);
    onchange?.(defaultValue);
  }
</script>

<div class="ops-slider" class:disabled data-testid={testid}>
  <div class="ops-slider__head">
    <span class="ops-slider__label" role="presentation" ondblclick={reset}>{label}</span>
    <NumberField {value} {min} {max} {step} {precision} {unit} ariaLabel={label} width={fieldWidth} {disabled} {oninput} {onchange} />
  </div>
  <div
    class="ops-slider__track"
    class:custom={!!track}
    class:dragging
    bind:this={trackEl}
    role="slider"
    tabindex={disabled ? -1 : 0}
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-disabled={disabled}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    ondblclick={reset}
    onkeydown={key}
  >
    <span class="rail" style:background={track}></span>
    {#if !track}
      <span class="fill" style:left="{Math.min(pos, opos) * 100}%" style:width="{Math.abs(pos - opos) * 100}%"></span>
    {/if}
    <span class="thumb" style:left="{pos * 100}%"></span>
  </div>
</div>

<style>
  .ops-slider {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ops-slider.disabled {
    opacity: 0.5;
  }
  .ops-slider__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: 22px;
  }
  .ops-slider__label {
    font: var(--type-caption);
    color: var(--text-body);
    user-select: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ops-slider__track {
    position: relative;
    height: 14px;
    cursor: pointer;
    touch-action: none;
    border-radius: var(--radius-xs);
    margin: 0 5px;
  }
  .ops-slider__track:focus-visible {
    box-shadow: none;
  }
  .ops-slider__track:focus-visible .thumb {
    box-shadow: 0 0 0 2px var(--border-focus);
  }
  .rail {
    position: absolute;
    left: -5px;
    right: -5px;
    top: 50%;
    height: 2px;
    transform: translateY(-50%);
    border-radius: 1px;
    background: var(--border-strong);
  }
  .custom .rail {
    height: 6px;
    border-radius: 3px;
    box-shadow: inset 0 0 0 1px var(--border-hairline);
  }
  .fill {
    position: absolute;
    top: 50%;
    height: 2px;
    transform: translateY(-50%);
    background: var(--text-muted);
  }
  .thumb {
    position: absolute;
    top: 50%;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--text-strong);
    box-shadow: 0 0 0 1px var(--bg-sunken);
    transition: transform var(--duration-instant) var(--ease-standard);
  }
  .ops-slider__track:hover .thumb,
  .dragging .thumb {
    transform: translate(-50%, -50%) scale(1.15);
  }
</style>
