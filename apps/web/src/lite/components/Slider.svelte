<script lang="ts">
  // A touch-friendly labelled slider. The fill grows from zero, so a
  // bipolar control reads as "how far, which way". Double-click or
  // double-tap resets it.
  import { t } from "../../lib/i18n";

  let {
    label,
    value,
    min = -100,
    max = 100,
    step = 1,
    defaultValue = 0,
    format = (v: number) => (v > 0 ? `+${Math.round(v)}` : `${Math.round(v)}`),
    disabled = false,
    emphasis = false,
    testid,
    oninput,
    oncommit,
  }: {
    label: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    defaultValue?: number;
    format?: (v: number) => string;
    disabled?: boolean;
    emphasis?: boolean;
    testid?: string;
    oninput: (v: number) => void;
    oncommit?: () => void;
  } = $props();

  const origin = $derived(Math.max(min, Math.min(max, 0)));
  const pct = (v: number) => ((v - min) / (max - min)) * 100;
  const from = $derived(Math.min(pct(origin), pct(value)));
  const to = $derived(Math.max(pct(origin), pct(value)));
  let lastTap = 0;

  function reset() {
    if (disabled || value === defaultValue) return;
    oninput(defaultValue);
    oncommit?.();
  }

  function onpointerdown(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    const now = performance.now();
    if (now - lastTap < 320) {
      e.preventDefault();
      reset();
      lastTap = 0;
    } else lastTap = now;
  }
</script>

<label class="slider" class:emphasis class:disabled class:changed={value !== defaultValue} ondblclick={reset}>
  <span class="head">
    <span class="name">{label}</span>
    <span class="value">{format(value)}</span>
  </span>
  <input
    type="range"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    data-testid={testid}
    aria-label={label}
    title={t("Double-click to reset")}
    style="--from: {from}%; --to: {to}%;"
    oninput={(e) => oninput(Number(e.currentTarget.value))}
    onchange={() => oncommit?.()}
    {onpointerdown}
  />
</label>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: 2px;
    user-select: none;
    -webkit-user-select: none;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
  }
  .name {
    font: var(--type-ui);
    font-weight: var(--weight-regular);
    color: var(--text-body);
  }
  .emphasis .name {
    font-weight: var(--weight-medium);
    color: var(--text-strong);
    font-size: var(--text-base);
  }
  .value {
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    transition: color var(--duration-fast) var(--ease-standard);
  }
  .changed .value {
    color: var(--text-strong);
  }
  .disabled {
    opacity: 0.5;
  }
  input {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: var(--control-h-sm);
    margin: 0;
    background: transparent;
    cursor: pointer;
    touch-action: pan-y;
  }
  input:disabled {
    cursor: not-allowed;
  }
  input:focus-visible {
    box-shadow: none;
  }
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: var(--radius-full);
    background: linear-gradient(
      to right,
      var(--border-hairline) 0 var(--from),
      var(--text-strong) var(--from) var(--to),
      var(--border-hairline) var(--to) 100%
    );
  }
  input::-moz-range-track {
    height: 4px;
    border-radius: var(--radius-full);
    background: linear-gradient(
      to right,
      var(--border-hairline) 0 var(--from),
      var(--text-strong) var(--from) var(--to),
      var(--border-hairline) var(--to) 100%
    );
  }
  input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 20px;
    height: 20px;
    margin-top: -8px;
    border-radius: 50%;
    background: var(--surface-card);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-sm);
    transition: transform var(--duration-fast) var(--ease-standard);
  }
  input::-moz-range-thumb {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--surface-card);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-sm);
  }
  input:active::-webkit-slider-thumb {
    transform: scale(1.12);
  }
  input:focus-visible::-webkit-slider-thumb {
    box-shadow: var(--focus-ring);
  }
  @media (pointer: coarse) {
    input::-webkit-slider-thumb {
      width: 26px;
      height: 26px;
      margin-top: -11px;
    }
  }
</style>
