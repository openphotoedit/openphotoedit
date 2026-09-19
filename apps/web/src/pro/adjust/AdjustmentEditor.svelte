<script lang="ts">
  // Controls for every adjustment kind. Used by the Properties panel (for
  // adjustment layers) and by Image › Adjustments dialogs (destructive).
  // `onchange(next, final)`: final is false while dragging.
  import { untrack } from "svelte";
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
  import Pipette from "@lucide/svelte/icons/pipette";
  import Plus from "@lucide/svelte/icons/plus";
  import Minus from "@lucide/svelte/icons/minus";
  import Pointer from "@lucide/svelte/icons/pointer";
  import IconButton from "../../ui/IconButton.svelte";
  import { newSeed, parseCube, withDefaults } from "../adjustments";
  import { armCanvasPick, sampleComposite, type PickPoint } from "./canvas-pick";
  import HueBandBar from "./HueBandBar.svelte";
  import { DEFAULT_BANDS, RANGE_HUES, bestRange, centered, exclude, hueOf, include, invert, type Band, type HueSatParams } from "./hue-bands";
  import { levelsFromSample, type Dropper, type LevelsValue } from "./levels-pick";
  import LevelsTrack from "./LevelsTrack.svelte";

  let {
    value,
    histogram = null,
    onchange,
    sample = (x: number, y: number) => sampleComposite(x, y, 3),
  }: {
    value: Adjustment;
    histogram?: HistogramData | null;
    onchange: (next: Adjustment, final: boolean) => void;
    /** Colour the adjustment receives at a document point (eyedroppers). */
    sample?: (x: number, y: number) => Promise<Rgba8 | null>;
  } = $props();

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
  const hsBands = $derived(((v.bands as Band[] | undefined) ?? DEFAULT_BANDS) as Band[]);
  const hsParams = $derived(v as unknown as HueSatParams);
  function setHs(key: string, val: number, final: boolean) {
    if (hsRange < 0) patch({ master: { ...snap(v.master), [key]: val } }, final);
    else {
      const ranges = snap(v.ranges) as Record<string, number>[];
      ranges[hsRange] = { ...ranges[hsRange], [key]: val };
      patch({ ranges }, final);
    }
  }
  function setBand(i: number, band: Band, final: boolean, extra: Record<string, unknown> = {}) {
    const bands = snap(hsBands).map((b) => [...b]) as Band[];
    bands[i] = band;
    patch({ bands, ...extra }, final);
  }

  // ---- Canvas eyedroppers and the targeted-adjust hand
  type Armed = "hs-sample" | "hs-add" | "hs-remove" | "hs-target" | "lv-black" | "lv-gray" | "lv-white";
  let armed = $state<Armed | null>(null);
  let pickNote = $state<string | null>(null);
  const toggleArm = (a: Armed) => ((armed = armed === a ? null : a), (pickNote = null));

  async function hueAt(p: PickPoint) {
    const c = await sample(p.x, p.y);
    const h = c ? hueOf(c.r, c.g, c.b) : null;
    pickNote = h ? null : t("That colour is grey, so it has no hue to pick. Click a coloured area.");
    return h?.hue ?? null;
  }

  // The targeted-adjust drag: the range is known only once the sample
  // arrives, so moves before that are remembered and applied then.
  let target: { x0: number; x: number; mod: boolean; range: number | null; base: { hue: number; saturation: number }; done: boolean } | null = null;
  function targetApply(final: boolean) {
    const tg = target;
    if (!tg || tg.range == null) return;
    const dx = (tg.x - tg.x0) * 0.5;
    const key = tg.mod ? "hue" : "saturation";
    const lim = tg.mod ? 180 : 100;
    const val = Math.round(Math.max(-lim, Math.min(lim, tg.base[key] + dx)));
    const ranges = snap(v.ranges) as Record<string, number>[];
    ranges[tg.range] = { ...ranges[tg.range], [key]: val };
    patch({ ranges }, final);
  }
  const pickHandlers = {
    down: async (p: PickPoint) => {
      const mode = armed;
      if (!mode) return;
      if (mode.startsWith("lv-")) {
        const c = await sample(p.x, p.y);
        if (!c) return;
        const next = levelsFromSample(snap(v) as unknown as LevelsValue, c, mode.slice(3) as Dropper);
        patch({ red: next.red, green: next.green, blue: next.blue }, true);
        return;
      }
      const tg: typeof target = mode === "hs-target" ? { x0: p.clientX, x: p.clientX, mod: p.mod, range: null, base: { hue: 0, saturation: 0 }, done: false } : null;
      if (tg) target = tg;
      const hue = await hueAt(p);
      if (hue == null) {
        if (tg) target = null;
        return;
      }
      const range = hsRange < 0 ? bestRange(hsBands, hue) : hsRange;
      hsRange = range;
      if (tg) {
        const r = v.ranges[range];
        tg.range = range;
        tg.base = { hue: r.hue, saturation: r.saturation };
        if (tg.done) {
          targetApply(true);
          target = null;
        } else if (tg.x !== tg.x0) targetApply(false);
        return;
      }
      const band = hsBands[range];
      setBand(range, mode === "hs-sample" ? centered(band, hue) : mode === "hs-add" ? include(band, hue) : exclude(band, hue), true);
    },
    move: (p: PickPoint) => {
      if (!target) return;
      target.x = p.clientX;
      target.mod = p.mod;
      targetApply(false);
    },
    up: (p: PickPoint) => {
      if (!target) return;
      target.x = p.clientX;
      if (target.range == null) {
        target.done = true;
        return;
      }
      targetApply(true);
      target = null;
    },
  };
  $effect(() => {
    const mode = armed;
    if (!mode) return;
    return untrack(() => armCanvasPick(pickHandlers, mode === "hs-target" ? "ew-resize" : "crosshair"));
  });
  $effect(() => {
    if (!armed) return;
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        armed = null;
      }
    };
    window.addEventListener("keydown", esc, true);
    return () => window.removeEventListener("keydown", esc, true);
  });
  // A different kind (or colorize) disarms the droppers.
  const armScope = $derived(`${v.kind}:${!!v.colorize}`);
  $effect(() => {
    void armScope;
    armed = null;
  });

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
    <div class="ops-row droppers" role="group" aria-label={t("Eyedroppers")}>
      {#each [["black", t("Sample in image to set black point")], ["gray", t("Sample in image to set gray point")], ["white", t("Sample in image to set white point")]] as [d, label] (d)}
        <IconButton size="sm" {label} pressed={armed === `lv-${d}`} testid="levels-dropper-{d}" onclick={() => toggleArm(`lv-${d}` as Armed)}>
          <span class="dropper"><Pipette size={14} /><span class="dot {d}"></span></span>
        </IconButton>
      {/each}
      {#if armed?.startsWith("lv-")}<span class="ops-note hint">{t("Click the image. Esc stops.")}</span>{/if}
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
    <div class="ops-row swatches" role="radiogroup" aria-label={t("Range")} data-testid="hue-range">
      <IconButton size="sm" label={t("Drag in the image to change saturation. Hold Ctrl or ⌘ to change hue.")} pressed={armed === "hs-target"} disabled={v.colorize} testid="hue-target" onclick={() => toggleArm("hs-target")}>
        <Pointer size={14} />
      </IconButton>
      <button type="button" role="radio" aria-checked={hsRange < 0} aria-label={t("Master")} title={t("Master")} class="swatch master" class:on={hsRange < 0} disabled={v.colorize} data-testid="hue-range-master" onclick={() => (hsRange = -1)}></button>
      {#each RANGES as label, i (i)}
        <button
          type="button"
          role="radio"
          aria-checked={hsRange === i}
          aria-label={label}
          title={label}
          class="swatch"
          class:on={hsRange === i}
          class:edited={v.ranges?.[i] && (v.ranges[i].hue || v.ranges[i].saturation || v.ranges[i].lightness)}
          style:--sw="hsl({RANGE_HUES[i]} 100% 50%)"
          disabled={v.colorize}
          data-testid="hue-range-{i}"
          onclick={() => (hsRange = i)}
        ></button>
      {/each}
    </div>
    {#if v.colorize}
      <Slider label={t("Hue")} value={v.colorize_hue} min={0} max={360} defaultValue={0} track="linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)" oninput={(x) => patch({ colorize_hue: x }, false)} onchange={(x) => patch({ colorize_hue: x }, true)} />
      <Slider label={t("Saturation")} value={v.colorize_saturation} min={0} max={100} defaultValue={25} oninput={(x) => patch({ colorize_saturation: x }, false)} onchange={(x) => patch({ colorize_saturation: x }, true)} />
      <Slider label={t("Lightness")} value={v.colorize_lightness} min={-100} max={100} defaultValue={0} track="linear-gradient(to right,#000,#888,#fff)" oninput={(x) => patch({ colorize_lightness: x }, false)} onchange={(x) => patch({ colorize_lightness: x }, true)} />
    {:else if hsCur}
      <Slider label={t("Hue")} value={hsCur.hue} min={-180} max={180} defaultValue={0} testid="hue-hue" track="linear-gradient(to right,#0ff,#00f,#f0f,#f00,#ff0,#0f0,#0ff)" oninput={(x) => setHs("hue", x, false)} onchange={(x) => setHs("hue", x, true)} />
      <Slider label={t("Saturation")} value={hsCur.saturation} min={-100} max={100} defaultValue={0} testid="hue-saturation" track="linear-gradient(to right,#808080,#f33)" oninput={(x) => setHs("saturation", x, false)} onchange={(x) => setHs("saturation", x, true)} />
      <Slider label={t("Lightness")} value={hsCur.lightness} min={-100} max={100} defaultValue={0} testid="hue-lightness" track="linear-gradient(to right,#000,#888,#fff)" oninput={(x) => setHs("lightness", x, false)} onchange={(x) => setHs("lightness", x, true)} />
    {/if}
    <div class="ops-row">
      <label class="ops-check"><input type="checkbox" checked={v.colorize} onchange={(e) => patch({ colorize: e.currentTarget.checked }, true)} />{t("Colorize")}</label>
      <span class="grow"></span>
      <IconButton size="sm" label={t("Sample a colour range from the image")} pressed={armed === "hs-sample"} disabled={v.colorize} testid="hue-dropper-sample" onclick={() => toggleArm("hs-sample")}>
        <Pipette size={14} />
      </IconButton>
      <IconButton size="sm" label={t("Add to the range")} pressed={armed === "hs-add"} disabled={v.colorize} testid="hue-dropper-add" onclick={() => toggleArm("hs-add")}>
        <span class="dropper"><Pipette size={14} /><Plus size={9} class="badge" /></span>
      </IconButton>
      <IconButton size="sm" label={t("Subtract from the range")} pressed={armed === "hs-remove"} disabled={v.colorize} testid="hue-dropper-remove" onclick={() => toggleArm("hs-remove")}>
        <span class="dropper"><Pipette size={14} /><Minus size={9} class="badge" /></span>
      </IconButton>
      <button
        type="button"
        class="oa-btn oa-btn--secondary small"
        disabled={v.colorize || hsRange < 0}
        data-testid="hue-invert"
        title={t("Swap the range for every hue outside it")}
        onclick={() => setBand(hsRange, invert(hsBands[hsRange]), true)}>{t("Invert")}</button
      >
    </div>
    {#if armed?.startsWith("hs-")}<p class="ops-note hint">{armed === "hs-target" ? t("Drag left or right in the image. Esc stops.") : t("Click a colour in the image. Esc stops.")}</p>{/if}
    {#if pickNote}<p class="ops-note hint" role="status">{pickNote}</p>{/if}
    {#if !v.colorize}
      <span class="ops-section-title">{t("Before – after")}</span>
    {/if}
    <HueBandBar params={hsParams} band={v.colorize || hsRange < 0 ? null : hsBands[hsRange]} rangeKey={v.colorize ? -2 : hsRange} onchange={(b, final) => setBand(hsRange, b, final)} />
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
  {:else if v.kind === "grain"}
    <Slider label={t("Amount")} value={v.amount} min={0} max={100} defaultValue={25} testid="grain-amount" oninput={(x) => patch({ amount: x }, false)} onchange={(x) => patch({ amount: x }, true)} />
    <Slider label={t("Size")} value={v.size} min={0.5} max={20} step={0.1} unit="px" defaultValue={1.5} testid="grain-size" oninput={(x) => patch({ size: x }, false)} onchange={(x) => patch({ size: x }, true)} />
    <Slider label={t("Roughness")} value={v.roughness} min={0} max={100} defaultValue={50} testid="grain-roughness" oninput={(x) => patch({ roughness: x }, false)} onchange={(x) => patch({ roughness: x }, true)} />
    <div class="ops-row">
      <span class="ops-note hint">{t("The grain is fixed to the image, so it looks the same at every zoom and in the export.")}</span>
      <button type="button" class="oa-btn oa-btn--secondary small" data-testid="grain-reseed" onclick={() => patch({ seed: newSeed() }, true)}>{t("New pattern")}</button>
    </div>
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
  .swatches {
    gap: 6px;
  }
  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border-radius: 50%;
    border: var(--border-width) solid var(--border-hairline);
    background: var(--sw);
    cursor: pointer;
    position: relative;
  }
  .swatch.master {
    background: conic-gradient(#f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
  }
  .swatch.on {
    outline: 2px solid var(--text-strong);
    outline-offset: 1px;
  }
  .swatch.edited::after {
    content: "";
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-strong);
  }
  .swatch:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .droppers {
    gap: 2px;
  }
  .dropper {
    position: relative;
    display: inline-flex;
  }
  .dropper :global(.badge) {
    position: absolute;
    right: -5px;
    bottom: -4px;
  }
  .dot {
    position: absolute;
    right: -4px;
    bottom: -3px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1px solid var(--text-muted);
  }
  .dot.black {
    background: #000;
  }
  .dot.gray {
    background: #808080;
  }
  .dot.white {
    background: #fff;
  }
  .hint {
    margin: 0;
    font: var(--type-caption);
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
