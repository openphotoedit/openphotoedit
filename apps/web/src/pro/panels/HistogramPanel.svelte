<script lang="ts">
  // A live histogram of the composite, refreshed at most every 300 ms.
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Histogram from "../../ui/Histogram.svelte";
  import Select from "../../ui/Select.svelte";
  import type { HistogramData } from "../../ui/histogram";
  import { histogram } from "../engine.svelte";

  let mode = $state<"colors" | "rgb" | "luminosity" | "red" | "green" | "blue">("colors");
  let data = $state<HistogramData | null>(null);
  let timer = 0;

  $effect(() => {
    void editor.summary?.revision;
    const has = editor.hasDocument;
    clearTimeout(timer);
    if (!has) {
      data = null;
      return;
    }
    timer = window.setTimeout(async () => {
      data = await histogram({});
    }, 300);
    return () => clearTimeout(timer);
  });

  const stats = $derived.by(() => {
    if (!data) return null;
    const arr = mode === "red" ? data.r : mode === "green" ? data.g : mode === "blue" ? data.b : data.l;
    let n = 0;
    let sum = 0;
    for (let i = 0; i < 256; i++) {
      n += arr[i];
      sum += arr[i] * i;
    }
    const mean = n ? sum / n : 0;
    let v = 0;
    for (let i = 0; i < 256; i++) v += arr[i] * (i - mean) ** 2;
    let acc = 0;
    let median = 0;
    for (let i = 0; i < 256; i++) {
      acc += arr[i];
      if (acc >= n / 2) {
        median = i;
        break;
      }
    }
    return { mean: mean.toFixed(2), dev: Math.sqrt(n ? v / n : 0).toFixed(2), median, clipLo: n ? ((arr[0] / n) * 100).toFixed(2) : "0", clipHi: n ? ((arr[255] / n) * 100).toFixed(2) : "0" };
  });
</script>

<div class="panel" data-testid="histogram-panel">
  <div class="ops-row">
    <span class="ops-label">{t("Channel")}</span>
    <span class="grow">
      <Select
        ariaLabel={t("Channel")}
        value={mode}
        onchange={(v) => (mode = v)}
        options={[
          { value: "colors", label: t("Colors") },
          { value: "rgb", label: "RGB" },
          { value: "luminosity", label: t("Luminosity") },
          null,
          { value: "red", label: t("Red") },
          { value: "green", label: t("Green") },
          { value: "blue", label: t("Blue") },
        ]}
      />
    </span>
  </div>
  <Histogram {data} {mode} height={100} />
  {#if stats}
    <dl class="stats">
      <dt>{t("Mean")}</dt><dd>{stats.mean}</dd>
      <dt>{t("Std dev")}</dt><dd>{stats.dev}</dd>
      <dt>{t("Median")}</dt><dd>{stats.median}</dd>
      <dt>{t("Clipped shadows")}</dt><dd>{stats.clipLo}%</dd>
      <dt>{t("Clipped highlights")}</dt><dd>{stats.clipHi}%</dd>
    </dl>
  {/if}
</div>

<style>
  .panel {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .stats {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 3px 8px;
    margin: 0;
    font: var(--type-caption);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    text-align: right;
    color: var(--text-body);
    font-variant-numeric: tabular-nums;
  }
</style>
