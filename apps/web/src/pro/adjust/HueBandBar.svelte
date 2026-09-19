<script lang="ts">
  // Photoshop's before/after spectrum under Hue/Saturation, with the four
  // band handles of the selected colour range: triangles are the falloff
  // ends, bars the edges of the full-strength range. Drag a handle to move
  // it, drag the range between the bars to move the whole band.
  import { t } from "../../lib/i18n";
  import { forward, middle, setHandle, shift, spectrum, type Band, type HueSatParams } from "./hue-bands";

  let {
    params,
    band = null,
    rangeKey = -1,
    onchange,
  }: {
    params: HueSatParams;
    /** The selected range's band; null for Master (no handles). */
    band?: Band | null;
    /** Changes when another range is selected: the bar re-centres. */
    rangeKey?: number;
    onchange?: (band: Band, final: boolean) => void;
  } = $props();

  // The bar starts 180° before the centre of the selected range (red for
  // Master, as in Photoshop). It stays put while a handle is dragged.
  let start = $state(180);
  let lastKey = NaN;
  $effect(() => {
    if (rangeKey !== lastKey) {
      lastKey = rangeKey;
      start = band ? Math.round(middle(band) - 180 + 360) % 360 : 180;
    }
  });

  const before = $derived(spectrum(start));
  const after = $derived(spectrum(start, params));
  const pos = (deg: number) => (forward(start, deg) / 360) * 100;

  let track: HTMLDivElement;
  let drag: { index: number; x0: number; band: Band } | null = null;

  function degAt(clientX: number) {
    const r = track.getBoundingClientRect();
    return start + ((clientX - r.left) / r.width) * 360;
  }

  function down(e: PointerEvent, index: number) {
    if (!band) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { index, x0: e.clientX, band: [...band] as Band };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const next = drag.index < 0 ? shift(drag.band, ((e.clientX - drag.x0) / track.getBoundingClientRect().width) * 360) : setHandle(drag.band, drag.index, degAt(e.clientX));
    if (next) onchange?.(next, false);
  }
  function up(e: PointerEvent) {
    if (!drag) return;
    const next = drag.index < 0 ? shift(drag.band, ((e.clientX - drag.x0) / track.getBoundingClientRect().width) * 360) : setHandle(drag.band, drag.index, degAt(e.clientX));
    drag = null;
    if (band) onchange?.(next ?? band, true);
  }

  // Shoulder and plateau segments; a segment may wrap past the bar's end.
  function segments(a: number, b: number): [number, number][] {
    const x0 = pos(a);
    const w = (forward(a, b) / 360) * 100;
    return x0 + w <= 100 ? [[x0, w]] : [[x0, 100 - x0], [0, x0 + w - 100]];
  }
  const round = (v: number) => Math.round(v);
</script>

<div class="hue-band" data-testid="hue-band-bar">
  <div class="bar" style:background={before} title={t("Before")}></div>
  <div class="bar" style:background={after} title={t("After")} data-testid="hue-after-bar"></div>
  <div class="track" role="group" aria-label={t("Range handles")} bind:this={track} class:off={!band} onpointermove={move} onpointerup={up} onpointercancel={up}>
    {#if band}
      {#each segments(band[0], band[1]) as [x, w], i (i)}
        <span class="shoulder" style:left="{x}%" style:width="{w}%"></span>
      {/each}
      {#each segments(band[2], band[3]) as [x, w], i (i)}
        <span class="shoulder" style:left="{x}%" style:width="{w}%"></span>
      {/each}
      {#each segments(band[1], band[2]) as [x, w], i (i)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="plateau" style:left="{x}%" style:width="{w}%" data-testid="hue-band-range" onpointerdown={(e) => down(e, -1)}></span>
      {/each}
      {#each [0, 1, 2, 3] as i (i)}
        <button
          type="button"
          class="handle"
          class:outer={i === 0 || i === 3}
          style:left="{pos(band[i])}%"
          aria-label={[t("Falloff start"), t("Range start"), t("Range end"), t("Falloff end")][i]}
          data-testid="hue-band-handle-{i}"
          onpointerdown={(e) => down(e, i)}
        ></button>
      {/each}
    {/if}
  </div>
  <div class="readout" data-testid="hue-band-readout">
    <span>{band ? `${round(band[0])}° / ${round(band[1])}°` : ""}</span>
    <span>{band ? `${round(band[2])}° / ${round(band[3])}°` : ""}</span>
  </div>
</div>

<style>
  .hue-band {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
    user-select: none;
  }
  .bar {
    height: 10px;
    border-radius: 1px;
  }
  .track {
    position: relative;
    height: 14px;
    margin-top: 2px;
    border-top: var(--border-width) solid var(--border-strong, var(--border-hairline));
    touch-action: none;
  }
  .track.off {
    opacity: 0.5;
  }
  .shoulder,
  .plateau {
    position: absolute;
    top: 0;
    height: 6px;
  }
  .shoulder {
    background: color-mix(in srgb, var(--text-muted) 35%, transparent);
  }
  .plateau {
    background: var(--text-muted);
    cursor: grab;
  }
  .handle {
    position: absolute;
    top: 0;
    width: 10px;
    height: 12px;
    margin-left: -5px;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: ew-resize;
  }
  .handle::after {
    content: "";
    position: absolute;
    left: 3px;
    top: 0;
    width: 4px;
    height: 10px;
    background: var(--text-strong);
    border-radius: 1px;
  }
  .handle.outer::after {
    left: 0;
    width: 0;
    height: 0;
    background: transparent;
    border-left: 5px solid transparent;
    border-right: 5px solid transparent;
    border-bottom: 8px solid var(--text-strong);
    border-radius: 0;
  }
  .handle:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 1px;
  }
  .readout {
    display: flex;
    justify-content: space-between;
    font: var(--type-caption);
    color: var(--text-muted);
    min-height: 14px;
  }
</style>
