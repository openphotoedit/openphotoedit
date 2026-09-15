<script lang="ts">
  // Controls for every adjustment kind. Used by the Properties panel (for
  // adjustment layers) and by Image › Adjustments dialogs (destructive).
  // `onchange(next, final)`: final is false while dragging.
  import type { Adjustment, GradientStop, Rgba8 } from "../../engine/types";
  import { t } from "../../lib/i18n";
  import { pickFiles } from "../../lib/io";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import ColorSwatch from "../../ui/ColorSwatch.svelte";
  import CurveEditor from "../../ui/CurveEditor.svelte";
  import GradientEditor from "../../ui/GradientEditor.svelte";
  import Histogram from "../../ui/Histogram.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";
  import Slider from "../../ui/Slider.svelte";
  import type { HistogramData } from "../../ui/histogram";
  import type { CurvePoint } from "../../ui/curve";
  import { parseCube, withDefaults } from "../adjustments";
  import LevelsTrack from "./LevelsTrack.svelte";

  let { value, histogram = null, onchange }: { value: Adjustment; histogram?: HistogramData | null; onchange: (next: Adjustment, final: boolean) => void } = $props();

  const v = $derived(withDefaults(value) as Record<string, any>);

  function snap<T>(x: T): T {
    return structuredClone($state.snapshot(x)) as T;
  }
  function patch(fields: Record<string, unknown>, final: boolean) {
    onchange({ ...snap(v), ...fields } as Adjustment, final);
  }

  // ---- Levels / Curves channel
  let channel = $state<"master" | "red" | "green" | "blue">("master");
  const channelOptions = [
    { value: "master" as const, label: "RGB" },
    { value: "red" as const, label: t("Red") },
    { value: "green" as const, label: t("Green") },
    { value: "blue" as const, label: t("Blue") },
  ];
  const histMode = $derived(channel === "master" ? "luminosity" : channel);

  // Levels gamma ↔ midpoint position: t_mid = 0.5^gamma.
  const gammaToMid = (g: number) => Math.pow(0.5, g);
  const midToGamma = (m: number) => Math.min(9.99, Math.max(0.01, Math.log(m) / Math.log(0.5)));

  function setLevels(fields: Record<string, number>, final: boolean) {
    const cur = snap(v[channel]);
    patch({ [channel]: { ...cur, ...fields } }, final);
  }

  function autoLevels() {
    if (!histogram) return;
    const clip = (arr: number[]) => {
      const total = arr.reduce((a, b) => a + b, 0) || 1;
      let acc = 0;
      let lo = 0;
      let hi = 255;
      for (let i = 0; i < 256; i++) if ((acc += arr[i]) / total > 0.001) { lo = i; break; }
      acc = 0;
      for (let i = 255; i >= 0; i--) if ((acc += arr[i]) / total > 0.001) { hi = i; break; }
      return { lo, hi: Math.max(hi, lo + 8) };
    };
    const ch = (arr: number[]) => {
      const c = clip(arr);
      return { in_black: c.lo, in_white: c.hi, gamma: 1, out_black: 0, out_white: 255 };
    };
    patch({ master: { in_black: 0, in_white: 255, gamma: 1, out_black: 0, out_white: 255 }, red: ch(histogram.r), green: ch(histogram.g), blue: ch(histogram.b) }, true);
  }

  // ---- Curves
  let curveSel = $state(-1);
  const CURVE_PRESETS: Record<string, CurvePoint[]> = {
    default: [[0, 0], [255, 255]],
    "medium-contrast": [[0, 0], [64, 54], [190, 202], [255, 255]],
    "strong-contrast": [[0, 0], [64, 44], [190, 212], [255, 255]],
    "linear-contrast": [[0, 0], [64, 58], [190, 197], [255, 255]],
    lighter: [[0, 0], [130, 150], [255, 255]],
    darker: [[0, 0], [128, 108], [255, 255]],
    negative: [[0, 255], [255, 0]],
    "cross-process": [[0, 20], [64, 50], [190, 210], [255, 240]],
  };
  const curvePts = $derived((v[channel] ?? [[0, 0], [255, 255]]) as CurvePoint[]);

  // ---- Hue/Saturation
  let hsRange = $state(-1);
  const RANGES = [t("Reds"), t("Yellows"), t("Greens"), t("Cyans"), t("Blues"), t("Magentas")];
  const hsCur = $derived(hsRange < 0 ? v.master : v.ranges?.[hsRange]);
  function setHs(key: string, val: number, final: boolean) {
    if (hsRange < 0) patch({ master: { ...snap(v.master), [key]: val } }, final);
    else {
      const ranges = snap(v.ranges) as Record<string, number>[];
      ranges[hsRange] = { ...ranges[hsRange], [key]: val };
      patch({ ranges }, final);
    }
  }

  // ---- Color balance
  let tone = $state<"shadows" | "midtones" | "highlights">("midtones");
  function setCb(i: number, val: number, final: boolean) {
    const arr = snap(v[tone]) as number[];
    arr[i] = val;
    patch({ [tone]: arr }, final);
  }

  // ---- Black & White
  const BW = [t("Reds"), t("Yellows"), t("Greens"), t("Cyans"), t("Blues"), t("Magentas")];
  const BW_TRACKS = ["#e33", "#ee3", "#3c3", "#3cc", "#35e", "#c3c"];

  // ---- Photo filter presets
  const PHOTO_FILTERS: [string, [number, number, number]][] = [
    [t("Warming Filter (85)"), [236, 138, 0]],
    [t("Warming Filter (LBA)"), [250, 150, 0]],
    [t("Warming Filter (81)"), [235, 177, 19]],
    [t("Cooling Filter (80)"), [0, 109, 255]],
    [t("Cooling Filter (LBB)"), [0, 93, 255]],
    [t("Cooling Filter (82)"), [0, 181, 255]],
    [t("Red"), [234, 26, 26]],
    [t("Orange"), [243, 132, 23]],
    [t("Yellow"), [249, 227, 28]],
    [t("Green"), [25, 201, 25]],
    [t("Cyan"), [29, 203, 234]],
    [t("Blue"), [29, 53, 234]],
    [t("Violet"), [155, 29, 234]],
    [t("Magenta"), [227, 24, 227]],
    [t("Sepia"), [172, 122, 51]],
    [t("Deep Red"), [255, 0, 0]],
    [t("Deep Blue"), [0, 34, 205]],
    [t("Deep Emerald"), [0, 140, 0]],
    [t("Deep Yellow"), [255, 213, 0]],
    [t("Underwater"), [0, 193, 177]],
  ];
  const pfPreset = $derived(PHOTO_FILTERS.findIndex(([, c]) => c[0] === v.color?.r && c[1] === v.color?.g && c[2] === v.color?.b));

  // ---- Channel mixer
  let mixOut = $state<"red" | "green" | "blue">("red");
  function setMix(i: number, val: number, final: boolean) {
    const row = snap(v[mixOut]) as number[];
    row[i] = val;
    patch({ [mixOut]: row }, final);
  }
  const mixTotal = $derived(((v[mixOut] as number[]) ?? [0, 0, 0]).slice(0, 3).reduce((a, b) => a + b, 0));

  // ---- Selective color
  let scColor = $state(0);
  const SC_COLORS = [t("Reds"), t("Yellows"), t("Greens"), t("Cyans"), t("Blues"), t("Magentas"), t("Whites"), t("Neutrals"), t("Blacks")];
  function setSc(i: number, val: number, final: boolean) {
    const colors = snap(v.colors) as number[][];
    colors[scColor][i] = val;
    patch({ colors }, final);
  }

  // ---- Gradient map presets
  const GM_PRESETS: [string, GradientStop[]][] = [
    [t("Black, White"), [{ pos: 0, color: { r: 0, g: 0, b: 0, a: 255 } }, { pos: 1, color: { r: 255, g: 255, b: 255, a: 255 } }]],
    [t("Violet, Orange"), [{ pos: 0, color: { r: 41, g: 10, b: 89, a: 255 } }, { pos: 1, color: { r: 255, g: 124, b: 0, a: 255 } }]],
    [t("Blue, Red, Yellow"), [{ pos: 0, color: { r: 10, g: 0, b: 178, a: 255 } }, { pos: 0.5, color: { r: 255, g: 0, b: 0, a: 255 } }, { pos: 1, color: { r: 255, g: 252, b: 0, a: 255 } }]],
    [t("Copper"), [{ pos: 0, color: { r: 16, g: 8, b: 4, a: 255 } }, { pos: 0.6, color: { r: 196, g: 112, b: 58, a: 255 } }, { pos: 1, color: { r: 255, g: 226, b: 188, a: 255 } }]],
    [t("Teal, Cream"), [{ pos: 0, color: { r: 8, g: 42, b: 52, a: 255 } }, { pos: 1, color: { r: 246, g: 232, b: 200, a: 255 } }]],
  ];

  // ---- Develop sections
  type Sl = { key: string; label: string; min: number; max: number; step?: number; track?: string };
  const DEVELOP: { title: string; sliders: Sl[] }[] = [
    {
      title: t("White balance"),
      sliders: [
        { key: "temperature", label: t("Temperature"), min: -100, max: 100, track: "linear-gradient(to right, #3a7bd5, #ddd, #e8b33a)" },
        { key: "tint", label: t("Tint"), min: -100, max: 100, track: "linear-gradient(to right, #3fbf5a, #ddd, #c44fc4)" },
      ],
    },
    {
      title: t("Light"),
      sliders: [
        { key: "exposure", label: t("Exposure"), min: -5, max: 5, step: 0.05 },
        { key: "contrast", label: t("Contrast"), min: -100, max: 100 },
        { key: "highlights", label: t("Highlights"), min: -100, max: 100 },
        { key: "shadows", label: t("Shadows"), min: -100, max: 100 },
        { key: "whites", label: t("Whites"), min: -100, max: 100 },
        { key: "blacks", label: t("Blacks"), min: -100, max: 100 },
      ],
    },
    {
      title: t("Presence"),
      sliders: [
        { key: "clarity", label: t("Clarity"), min: -100, max: 100 },
        { key: "dehaze", label: t("Dehaze"), min: -100, max: 100 },
        { key: "vibrance", label: t("Vibrance"), min: -100, max: 100 },
        { key: "saturation", label: t("Saturation"), min: -100, max: 100 },
      ],
    },
    {
      title: t("Effects"),
      sliders: [
        { key: "vignette", label: t("Vignette"), min: -100, max: 100 },
        { key: "grain", label: t("Grain"), min: 0, max: 100 },
        { key: "black_white", label: t("Black & white"), min: 0, max: 100 },
      ],
    },
  ];

  let lutError = $state<string | null>(null);
  async function importCube() {
    const [f] = await pickFiles(".cube");
    if (!f) return;
    try {
      const lut = parseCube(f.name.replace(/\.cube$/i, ""), await f.text());
      lutError = null;
      patch({ name: lut.name, size: lut.size, table: lut.table }, true);
    } catch (e) {
      lutError = e instanceof Error ? e.message : String(e);
    }
  }

  let bwTintOpen = $state(false);
</script>

<div class="adj-editor" data-kind={v.kind} data-testid="adjustment-editor">
  {#if v.kind === "brightness-contrast"}
    <Slider label={t("Brightness")} value={v.brightness} min={-150} max={150} defaultValue={0} testid="adj-brightness" oninput={(x) => patch({ brightness: x }, false)} onchange={(x) => patch({ brightness: x }, true)} />
    <Slider label={t("Contrast")} value={v.contrast} min={-50} max={100} defaultValue={0} testid="adj-contrast" oninput={(x) => patch({ contrast: x }, false)} onchange={(x) => patch({ contrast: x }, true)} />
    <label class="ops-check"><input type="checkbox" checked={v.legacy} onchange={(e) => patch({ legacy: e.currentTarget.checked }, true)} />{t("Use legacy")}</label>
  {:else if v.kind === "levels"}
    <div class="ops-row">
      <SegmentedControl ariaLabel={t("Channel")} options={channelOptions} value={channel} onchange={(c) => (channel = c)} />
      <span class="grow"></span>
      <button type="button" class="oa-btn oa-btn--secondary small" disabled={!histogram} onclick={autoLevels}>{t("Auto")}</button>
    </div>
    <Histogram data={histogram} mode={histMode} height={96} />
    <LevelsTrack
      label={t("Input")}
      testid="levels-input"
      lo={v[channel].in_black}
      hi={v[channel].in_white}
      mid={gammaToMid(v[channel].gamma)}
      onchange={(x, final) => setLevels({ in_black: x.lo, in_white: x.hi, gamma: midToGamma(x.mid ?? 0.5) }, final)}
    />
    <div class="triple">
      <NumberField value={v[channel].in_black} min={0} max={253} width={44} ariaLabel={t("Input black")} onchange={(x) => setLevels({ in_black: x }, true)} />
      <NumberField value={v[channel].gamma} min={0.01} max={9.99} step={0.01} width={48} ariaLabel={t("Gamma")} onchange={(x) => setLevels({ gamma: x }, true)} />
      <NumberField value={v[channel].in_white} min={2} max={255} width={44} ariaLabel={t("Input white")} onchange={(x) => setLevels({ in_white: x }, true)} />
    </div>
    <span class="ops-section-title">{t("Output levels")}</span>
    <LevelsTrack label={t("Output")} lo={v[channel].out_black} hi={v[channel].out_white} onchange={(x, final) => setLevels({ out_black: x.lo, out_white: x.hi }, final)} />
    <div class="triple two">
      <NumberField value={v[channel].out_black} min={0} max={255} width={44} ariaLabel={t("Output black")} onchange={(x) => setLevels({ out_black: x }, true)} />
      <NumberField value={v[channel].out_white} min={0} max={255} width={44} ariaLabel={t("Output white")} onchange={(x) => setLevels({ out_white: x }, true)} />
    </div>
  {:else if v.kind === "curves"}
    <div class="ops-row">
      <span class="grow">
        <Select
          ariaLabel={t("Preset")}
          value={"custom"}
          options={[
            { value: "custom", label: t("Custom") },
            null,
            { value: "default", label: t("Default") },
            { value: "medium-contrast", label: t("Medium contrast") },
            { value: "strong-contrast", label: t("Strong contrast") },
            { value: "linear-contrast", label: t("Linear contrast") },
            { value: "lighter", label: t("Lighter") },
            { value: "darker", label: t("Darker") },
            { value: "negative", label: t("Negative") },
            { value: "cross-process", label: t("Cross process") },
          ]}
          onchange={(p) => {
            if (p !== "custom") patch({ [channel]: CURVE_PRESETS[p] }, true);
          }}
        />
      </span>
      <SegmentedControl ariaLabel={t("Channel")} size="xs" options={channelOptions} value={channel} onchange={(c) => ((channel = c), (curveSel = -1))} />
    </div>
    <CurveEditor
      points={curvePts}
      color={channel === "master" ? "neutral" : channel}
      {histogram}
      histogramChannel={histMode}
      bind:selected={curveSel}
      testid="curve-editor"
      onchange={(pts, final) => patch({ [channel]: pts }, final)}
    />
    <div class="ops-row">
      <NumberField
        label={t("Input")}
        value={curveSel >= 0 && curvePts[curveSel] ? curvePts[curveSel][0] : 0}
        min={0}
        max={255}
        width={44}
        disabled={curveSel < 0}
        onchange={(x) => patch({ [channel]: curvePts.map((p, i) => (i === curveSel ? [x, p[1]] : p)) }, true)}
      />
      <NumberField
        label={t("Output")}
        value={curveSel >= 0 && curvePts[curveSel] ? curvePts[curveSel][1] : 0}
        min={0}
        max={255}
        width={44}
        disabled={curveSel < 0}
        onchange={(x) => patch({ [channel]: curvePts.map((p, i) => (i === curveSel ? [p[0], x] : p)) }, true)}
      />
      <span class="grow"></span>
      <button type="button" class="oa-btn oa-btn--ghost small" onclick={() => patch({ [channel]: [[0, 0], [255, 255]] }, true)}>{t("Reset")}</button>
    </div>
  {:else if v.kind === "exposure"}
    <Slider label={t("Exposure")} value={v.exposure} min={-20} max={20} step={0.01} defaultValue={0} oninput={(x) => patch({ exposure: x }, false)} onchange={(x) => patch({ exposure: x }, true)} />
    <Slider label={t("Offset")} value={v.offset} min={-0.5} max={0.5} step={0.0001} defaultValue={0} fieldWidth={60} oninput={(x) => patch({ offset: x }, false)} onchange={(x) => patch({ offset: x }, true)} />
    <Slider label={t("Gamma correction")} value={v.gamma} min={0.01} max={9.99} step={0.01} defaultValue={1} origin={1} oninput={(x) => patch({ gamma: x }, false)} onchange={(x) => patch({ gamma: x }, true)} />
  {:else if v.kind === "vibrance"}
    <Slider label={t("Vibrance")} value={v.vibrance} min={-100} max={100} defaultValue={0} oninput={(x) => patch({ vibrance: x }, false)} onchange={(x) => patch({ vibrance: x }, true)} />
    <Slider label={t("Saturation")} value={v.saturation} min={-100} max={100} defaultValue={0} oninput={(x) => patch({ saturation: x }, false)} onchange={(x) => patch({ saturation: x }, true)} />
  {:else if v.kind === "hue-saturation"}
    <div class="ops-row">
      <span class="ops-label">{t("Range")}</span>
      <span class="grow">
        <Select ariaLabel={t("Range")} value={hsRange} disabled={v.colorize} options={[{ value: -1, label: t("Master") }, null, ...RANGES.map((r, i) => ({ value: i, label: r }))]} onchange={(r) => (hsRange = r)} testid="hue-range" />
      </span>
    </div>
    {#if v.colorize}
      <Slider label={t("Hue")} value={v.colorize_hue} min={0} max={360} defaultValue={0} track="linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)" oninput={(x) => patch({ colorize_hue: x }, false)} onchange={(x) => patch({ colorize_hue: x }, true)} />
      <Slider label={t("Saturation")} value={v.colorize_saturation} min={0} max={100} defaultValue={25} oninput={(x) => patch({ colorize_saturation: x }, false)} onchange={(x) => patch({ colorize_saturation: x }, true)} />
      <Slider label={t("Lightness")} value={v.colorize_lightness} min={-100} max={100} defaultValue={0} track="linear-gradient(to right,#000,#888,#fff)" oninput={(x) => patch({ colorize_lightness: x }, false)} onchange={(x) => patch({ colorize_lightness: x }, true)} />
    {:else if hsCur}
      <Slider label={t("Hue")} value={hsCur.hue} min={-180} max={180} defaultValue={0} testid="hue-hue" track="linear-gradient(to right,#0ff,#00f,#f0f,#f00,#ff0,#0f0,#0ff)" oninput={(x) => setHs("hue", x, false)} onchange={(x) => setHs("hue", x, true)} />
      <Slider label={t("Saturation")} value={hsCur.saturation} min={-100} max={100} defaultValue={0} testid="hue-saturation" track="linear-gradient(to right,#808080,#f33)" oninput={(x) => setHs("saturation", x, false)} onchange={(x) => setHs("saturation", x, true)} />
      <Slider label={t("Lightness")} value={hsCur.lightness} min={-100} max={100} defaultValue={0} track="linear-gradient(to right,#000,#888,#fff)" oninput={(x) => setHs("lightness", x, false)} onchange={(x) => setHs("lightness", x, true)} />
    {/if}
    <label class="ops-check"><input type="checkbox" checked={v.colorize} onchange={(e) => patch({ colorize: e.currentTarget.checked }, true)} />{t("Colorize")}</label>
    <div class="rainbow" aria-hidden="true">
      <span style:background="linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)"></span>
      <span
        style:background="linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)"
        style:filter="hue-rotate({v.colorize ? 0 : (hsCur?.hue ?? 0)}deg) saturate({v.colorize ? 0.3 : 1 + (hsCur?.saturation ?? 0) / 100})"
      ></span>
    </div>
  {:else if v.kind === "color-balance"}
    <SegmentedControl
      ariaLabel={t("Tone")}
      options={[
        { value: "shadows", label: t("Shadows") },
        { value: "midtones", label: t("Midtones") },
        { value: "highlights", label: t("Highlights") },
      ]}
      value={tone}
      onchange={(x) => (tone = x)}
    />
    {#each [[t("Cyan"), t("Red"), "linear-gradient(to right,#0cc,#ccc,#e33)"], [t("Magenta"), t("Green"), "linear-gradient(to right,#c3c,#ccc,#3c3)"], [t("Yellow"), t("Blue"), "linear-gradient(to right,#dd3,#ccc,#35e)"]] as [a, b, track], i (i)}
      <Slider label="{a} – {b}" value={v[tone][i]} min={-100} max={100} defaultValue={0} {track} oninput={(x) => setCb(i, x, false)} onchange={(x) => setCb(i, x, true)} />
    {/each}
    <label class="ops-check"><input type="checkbox" checked={v.preserve_luminosity} onchange={(e) => patch({ preserve_luminosity: e.currentTarget.checked }, true)} />{t("Preserve luminosity")}</label>
  {:else if v.kind === "black-white"}
    {#each BW as label, i (i)}
      <Slider
        {label}
        value={v.weights[i]}
        min={-200}
        max={300}
        unit="%"
        defaultValue={[40, 60, 40, 60, 20, 80][i]}
        track="linear-gradient(to right,#000,{BW_TRACKS[i]},#fff)"
        oninput={(x) => patch({ weights: v.weights.map((w: number, j: number) => (j === i ? x : w)) }, false)}
        onchange={(x) => patch({ weights: v.weights.map((w: number, j: number) => (j === i ? x : w)) }, true)}
      />
    {/each}
    <div class="ops-row">
      <label class="ops-check"><input type="checkbox" checked={v.tint} onchange={(e) => patch({ tint: e.currentTarget.checked }, true)} />{t("Tint")}</label>
      <ColorSwatch color={v.tint_color} size={18} label={t("Tint colour")} onclick={() => (bwTintOpen = !bwTintOpen)} />
    </div>
    {#if bwTintOpen}
      <ColorPicker value={v.tint_color} height={80} oninput={(c) => patch({ tint_color: c, tint: true }, false)} onchange={(c) => patch({ tint_color: c, tint: true }, true)} />
    {/if}
  {:else if v.kind === "photo-filter"}
    <div class="ops-row">
      <span class="ops-label">{t("Filter")}</span>
      <span class="grow">
        <Select
          ariaLabel={t("Filter")}
          value={pfPreset}
          options={[{ value: -1, label: t("Custom colour") }, null, ...PHOTO_FILTERS.map(([label], i) => ({ value: i, label }))]}
          onchange={(i) => {
            if (i >= 0) {
              const [r, g, b] = PHOTO_FILTERS[i][1];
              patch({ color: { r, g, b, a: 255 } }, true);
            }
          }}
        />
      </span>
    </div>
    <ColorPicker value={v.color as Rgba8} height={70} showFields={false} oninput={(c) => patch({ color: c }, false)} onchange={(c) => patch({ color: c }, true)} />
    <Slider label={t("Density")} value={v.density} min={0} max={100} unit="%" defaultValue={25} oninput={(x) => patch({ density: x }, false)} onchange={(x) => patch({ density: x }, true)} />
    <label class="ops-check"><input type="checkbox" checked={v.preserve_luminosity} onchange={(e) => patch({ preserve_luminosity: e.currentTarget.checked }, true)} />{t("Preserve luminosity")}</label>
  {:else if v.kind === "channel-mixer"}
    <div class="ops-row">
      <span class="ops-label">{t("Output channel")}</span>
      <span class="grow">
        <Select
          ariaLabel={t("Output channel")}
          value={v.monochrome ? "red" : mixOut}
          disabled={v.monochrome}
          options={[
            { value: "red", label: t("Red") },
            { value: "green", label: t("Green") },
            { value: "blue", label: t("Blue") },
          ]}
          onchange={(x) => (mixOut = x)}
        />
      </span>
    </div>
    <label class="ops-check"><input type="checkbox" checked={v.monochrome} onchange={(e) => ((mixOut = "red"), patch({ monochrome: e.currentTarget.checked }, true))} />{t("Monochrome")}</label>
    {#each [t("Red"), t("Green"), t("Blue")] as label, i (i)}
      <Slider {label} value={v[mixOut][i]} min={-200} max={200} unit="%" defaultValue={mixOut === ["red", "green", "blue"][i] ? 100 : 0} oninput={(x) => setMix(i, x, false)} onchange={(x) => setMix(i, x, true)} />
    {/each}
    <p class="total" class:warn={mixTotal > 100}>{t("Total")}: {mixTotal > 0 ? "+" : ""}{mixTotal}%</p>
    <Slider label={t("Constant")} value={v[mixOut][3]} min={-200} max={200} unit="%" defaultValue={0} oninput={(x) => setMix(3, x, false)} onchange={(x) => setMix(3, x, true)} />
  {:else if v.kind === "invert"}
    <p class="ops-note">{t("Invert has no settings. It reverses every colour below this layer.")}</p>
  {:else if v.kind === "posterize"}
    <Slider label={t("Levels")} value={v.levels} min={2} max={255} defaultValue={4} oninput={(x) => patch({ levels: x }, false)} onchange={(x) => patch({ levels: x }, true)} />
  {:else if v.kind === "threshold"}
    <Histogram data={histogram} mode="luminosity" height={96}>
      <span class="marker" style:left="{(v.level / 255) * 100}%"></span>
    </Histogram>
    <Slider label={t("Threshold level")} value={v.level} min={1} max={255} defaultValue={128} track="linear-gradient(to right,#000,#fff)" oninput={(x) => patch({ level: x }, false)} onchange={(x) => patch({ level: x }, true)} />
  {:else if v.kind === "gradient-map"}
    <div class="ops-row">
      <span class="ops-label">{t("Preset")}</span>
      <span class="grow">
        <Select ariaLabel={t("Preset")} value={-1} options={[{ value: -1, label: t("Choose a preset") }, null, ...GM_PRESETS.map(([label], i) => ({ value: i, label }))]} onchange={(i) => i >= 0 && patch({ stops: structuredClone(GM_PRESETS[i][1]) }, true)} />
      </span>
    </div>
    <GradientEditor stops={v.stops} onchange={(stops, final) => patch({ stops }, final)} />
    <label class="ops-check"><input type="checkbox" checked={v.reverse} onchange={(e) => patch({ reverse: e.currentTarget.checked }, true)} />{t("Reverse")}</label>
  {:else if v.kind === "selective-color"}
    <div class="ops-row">
      <span class="ops-label">{t("Colors")}</span>
      <span class="grow">
        <Select ariaLabel={t("Colors")} value={scColor} options={SC_COLORS.map((label, i) => ({ value: i, label }))} onchange={(i) => (scColor = i)} />
      </span>
    </div>
    {#each [t("Cyan"), t("Magenta"), t("Yellow"), t("Black")] as label, i (i)}
      <Slider {label} value={v.colors[scColor][i]} min={-100} max={100} unit="%" defaultValue={0} oninput={(x) => setSc(i, x, false)} onchange={(x) => setSc(i, x, true)} />
    {/each}
    <SegmentedControl
      ariaLabel={t("Method")}
      options={[
        { value: "relative", label: t("Relative") },
        { value: "absolute", label: t("Absolute") },
      ]}
      value={v.absolute ? "absolute" : "relative"}
      onchange={(m) => patch({ absolute: m === "absolute" }, true)}
    />
  {:else if v.kind === "develop"}
    {#each DEVELOP as section (section.title)}
      <div class="section">
        <span class="ops-section-title">{section.title}</span>
        {#each section.sliders as sl (sl.key)}
          <Slider label={sl.label} value={v[sl.key]} min={sl.min} max={sl.max} step={sl.step ?? 1} defaultValue={0} track={sl.track} oninput={(x) => patch({ [sl.key]: x }, false)} onchange={(x) => patch({ [sl.key]: x }, true)} />
        {/each}
      </div>
    {/each}
  {:else if v.kind === "color-lookup"}
    <div class="ops-row">
      <span class="ops-label">{t("LUT")}</span>
      <span class="lut-name">{v.name}{v.size > 2 ? ` · ${v.size}³` : ""}</span>
      <span class="grow"></span>
      <button type="button" class="oa-btn oa-btn--secondary small" onclick={importCube}>{t("Load .cube…")}</button>
    </div>
    {#if lutError}<p class="ops-error">{lutError}</p>{/if}
    <Slider label={t("Strength")} value={Math.round((v.strength ?? 1) * 100)} min={0} max={100} unit="%" defaultValue={100} oninput={(x) => patch({ strength: x / 100 }, false)} onchange={(x) => patch({ strength: x / 100 }, true)} />
  {:else}
    <p class="ops-note">{t("This adjustment has no editor yet.")}</p>
  {/if}
</div>

<style>
  .adj-editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .grow > :global(*) {
    flex: 1;
  }
  .small {
    height: 22px;
    padding: 0 8px;
    font-size: var(--text-xs);
  }
  .triple {
    display: flex;
    justify-content: space-between;
  }
  .rainbow {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
  }
  .rainbow span {
    height: 6px;
    border-radius: 1px;
  }
  .total {
    margin: -2px 0 0;
    font: var(--type-caption);
    color: var(--text-muted);
    text-align: right;
  }
  .total.warn {
    color: var(--warning-fg);
  }
  .marker {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--text-strong);
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 6px;
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .section:last-child {
    border-bottom: 0;
  }
  .lut-name {
    font: var(--type-caption);
    color: var(--text-strong);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
