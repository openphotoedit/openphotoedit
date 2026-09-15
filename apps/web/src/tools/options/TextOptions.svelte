<script lang="ts">
  import AlignLeft from "@lucide/svelte/icons/align-left";
  import AlignCenter from "@lucide/svelte/icons/align-center";
  import AlignRight from "@lucide/svelte/icons/align-right";
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { toolSettings } from "../settings.svelte";
  import { toolState } from "../state.svelte";
  import { cancelText, commitText } from "../text.svelte";
  import Choice from "./Choice.svelte";
  import Color from "./Color.svelte";
  import Range from "./Range.svelte";
  import Row from "./Row.svelte";
  import Segmented from "./Segmented.svelte";
  import Toggle from "./Toggle.svelte";

  const FONTS = [
    { value: "Inter, system-ui, sans-serif", label: "Sans serif" },
    { value: "Geist, system-ui, sans-serif", label: "Geist" },
    { value: "Georgia, 'Times New Roman', serif", label: "Serif" },
    { value: "'Helvetica Neue', Helvetica, Arial, sans-serif", label: "Helvetica" },
    { value: "'Courier New', ui-monospace, monospace", label: "Monospace" },
    { value: "Impact, 'Arial Black', sans-serif", label: "Impact" },
    { value: "'Comic Sans MS', 'Chalkboard SE', cursive", label: "Handwriting" },
  ];
  const fonts = $derived(FONTS.some((f) => f.value === toolSettings.fontFamily) ? FONTS : [{ value: toolSettings.fontFamily, label: toolSettings.fontFamily.split(",")[0].replace(/'/g, "") }, ...FONTS]);
  const weights = [
    { value: 300, label: t("Light") },
    { value: 400, label: t("Regular") },
    { value: 600, label: t("Semibold") },
    { value: 800, label: t("Bold") },
  ];
  const aligns = [
    { value: "left" as const, label: t("Align left"), icon: AlignLeft },
    { value: "center" as const, label: t("Align centre"), icon: AlignCenter },
    { value: "right" as const, label: t("Align right"), icon: AlignRight },
  ];
</script>

<Row testid="options-text">
  <Choice label={t("Font")} bind:value={toolSettings.fontFamily} options={fonts} />
  <Choice label={t("Weight")} bind:value={toolSettings.fontWeight} options={weights} />
  <Toggle label={t("Italic")} bind:checked={toolSettings.italic} />
  <Range label={t("Size")} bind:value={toolSettings.fontSize} min={4} max={1000} unit="px" testid="text-size" />
  <Color label={t("Colour")} bind:value={toolSettings.textColor} fallback={editor.primary} testid="text-color" />
  <Segmented bind:value={toolSettings.textAlign} options={aligns} />
  <Range label={t("Line height")} bind:value={toolSettings.lineHeight} min={0.5} max={3} step={0.05} />
  <Range label={t("Tracking")} bind:value={toolSettings.letterSpacing} min={-20} max={100} unit="px" />
  <span class="ops-topt__sep"></span>
  <Toggle label={t("Background")} bind:checked={toolSettings.textBackgroundOn} testid="text-background" />
  {#if toolSettings.textBackgroundOn}
    <Color label={t("Background colour")} bind:value={toolSettings.textBackground} />
    <Range label={t("Padding")} bind:value={toolSettings.textPadding} min={0} max={200} unit="px" />
  {/if}
  <Toggle label={t("Outline")} bind:checked={toolSettings.textStrokeOn} />
  {#if toolSettings.textStrokeOn}
    <Color label={t("Outline colour")} bind:value={toolSettings.textStroke} />
    <Range label={t("Outline width")} bind:value={toolSettings.textStrokeWidth} min={1} max={50} unit="px" />
  {/if}
  {#if toolState.textEditing}
    <span class="ops-topt__sep"></span>
    <button class="oa-btn oa-btn--ghost" type="button" onmousedown={(e) => e.preventDefault()} onclick={() => cancelText(editor)}><X size={16} />{t("Cancel")}</button>
    <button class="oa-btn oa-btn--primary" type="button" onmousedown={(e) => e.preventDefault()} onclick={() => commitText(editor)} data-testid="text-commit"><Check size={16} />{t("Done")}</button>
  {/if}
</Row>
