<script lang="ts">
  import MoveUpRight from "@lucide/svelte/icons/move-up-right";
  import Square from "@lucide/svelte/icons/square";
  import Circle from "@lucide/svelte/icons/circle";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Highlighter from "@lucide/svelte/icons/highlighter";
  import Type from "@lucide/svelte/icons/type";
  import RectangleHorizontal from "@lucide/svelte/icons/rectangle-horizontal";
  import Grid3x3 from "@lucide/svelte/icons/grid-3x3";
  import Slash from "@lucide/svelte/icons/slash";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Plus from "@lucide/svelte/icons/plus";
  import Check from "@lucide/svelte/icons/check";
  import { t } from "../../lib/i18n";
  import { editor } from "../../lib/editor.svelte";
  import { LITE_MARKUP_TOOLS } from "../../tools/groups";
  import { toolState } from "../../tools/state.svelte";
  import { commitText, textEdit } from "../../tools/text.svelte";
  import { lite } from "../lite.svelte";
  import { activateTop, liteTools, MARKUP_TOOL_IDS, rgbaToHex, SWATCHES, type Size } from "../tools.svelte";

  const ICONS: Record<string, typeof Square> = {
    arrow: MoveUpRight,
    "shape-rect": Square,
    "shape-ellipse": Circle,
    "shape-line": Slash,
    pen: PenLine,
    highlighter: Highlighter,
    text: Type,
    redact: RectangleHorizontal,
    pixelate: Grid3x3,
  };

  const LABELS: Record<string, string> = Object.fromEntries([...LITE_MARKUP_TOOLS.map((x) => [x.id, x.label]), ["pixelate", "Pixelate"]]);

  const HINTS: Record<string, string> = {
    arrow: "Drag from where the arrow starts to where it points.",
    "shape-rect": "Drag to draw a box.",
    "shape-ellipse": "Drag to draw a circle or an ellipse.",
    text: "Tap where the text should go, then type.",
    pen: "Draw freehand.",
    highlighter: "Drag over what should stand out.",
    redact: "Drag over anything private. It is covered solid black.",
    pixelate: "Drag over faces or text. The pixels underneath are destroyed, not hidden.",
  };

  const SIZES: { id: Size; label: string }[] = [
    { id: "s", label: "Small" },
    { id: "m", label: "Medium" },
    { id: "l", label: "Large" },
  ];

  const current = $derived(liteTools.markup);
  const selectedId = $derived(toolState.annotationSelected);
  const selected = $derived.by(() => {
    void editor.summary?.revision;
    return selectedId == null ? null : (lite.layers().find((l) => l.id === selectedId) ?? null);
  });
  const isText = $derived(current === "text");
  const showWidth = $derived(["arrow", "shape-rect", "shape-ellipse", "shape-line", "pen", "highlighter"].includes(current));
  const showColor = $derived(current !== "redact" && current !== "pixelate");

  let customInput = $state<HTMLInputElement>();

  function pick(id: string) {
    liteTools.markup = id;
    editor.tool = id;
    void activateTop();
  }

  // Selecting a shape adopts its colour, so the row shows what it is.
  $effect(() => {
    const c = selected?.shape?.stroke;
    if (c) liteTools.color = rgbaToHex(c);
  });

  async function remove() {
    const id = toolState.annotationSelected;
    if (id == null) return;
    toolState.annotationSelected = null;
    await lite.exec({ op: "layer.delete", ids: [id] });
    lite.note(t("Delete markup"));
  }
</script>

<div class="markup">
  <section class="lt-section">
    <div class="lt-tools">
      {#each MARKUP_TOOL_IDS as id (id)}
        {@const Icon = ICONS[id] ?? Square}
        <button class="lt-tool" aria-pressed={current === id} data-testid="markup-{id}" onclick={() => pick(id)}>
          <Icon size={20} />
          <span>{t(LABELS[id] ?? id)}</span>
        </button>
      {/each}
    </div>
    {#if textEdit.session}
      <div class="selected" data-testid="text-editing">
        <span class="lt-hint">{t("Type your text, then tap Done.")}</span>
        <button class="oa-btn oa-btn--primary" data-testid="text-done" onclick={() => commitText(editor)}><Check size={14} />{t("Done")}</button>
      </div>
    {:else if selected}
      <div class="selected" data-testid="markup-selected">
        <span class="lt-hint">{t("Drag to move it, or drag a handle to resize.")}</span>
        <button class="oa-btn oa-btn--secondary" data-testid="markup-delete" onclick={remove}><Trash2 size={14} />{t("Delete")}</button>
      </div>
    {:else}
      <p class="lt-hint">{t(HINTS[current] ?? "")}</p>
    {/if}
  </section>

  {#if showColor}
    <section class="lt-section">
      <h3 class="lt-eyebrow">{t("Color")}</h3>
      <div class="lt-swatches">
        {#each SWATCHES as hex (hex)}
          <button class="lt-swatch" style:background={hex} aria-label={hex} aria-pressed={liteTools.color.toLowerCase() === hex} onclick={() => liteTools.applyColor(hex)}></button>
        {/each}
        <button
          class="lt-swatch custom"
          aria-label={t("Custom color")}
          aria-pressed={!SWATCHES.includes(liteTools.color.toLowerCase())}
          style:--c={liteTools.color}
          onclick={() => customInput?.click()}
        >
          <Plus size={16} />
        </button>
        <input class="visually-hidden" type="color" bind:this={customInput} value={liteTools.color} oninput={(e) => liteTools.applyColor(e.currentTarget.value)} tabindex="-1" />
      </div>
    </section>
  {/if}

  {#if showWidth}
    <section class="lt-section">
      <h3 class="lt-eyebrow">{t("Line width")}</h3>
      <div class="lt-seg" role="group" aria-label={t("Line width")}>
        {#each SIZES as s (s.id)}
          <button aria-pressed={liteTools.width === s.id} aria-label={t(s.label)} onclick={() => liteTools.applyWidth(s.id)}>
            <span class="bar" style:height="{s.id === 's' ? 2 : s.id === 'm' ? 4 : 7}px"></span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if isText}
    <section class="lt-section">
      <h3 class="lt-eyebrow">{t("Text size")}</h3>
      <div class="lt-seg" role="group" aria-label={t("Text size")}>
        {#each SIZES as s (s.id)}
          <button aria-pressed={liteTools.textSize === s.id} aria-label={t(s.label)} onclick={() => liteTools.applyTextSize(s.id)}>
            <span style:font-size="{s.id === 's' ? 12 : s.id === 'm' ? 15 : 19}px">Aa</span>
          </button>
        {/each}
      </div>
      <label class="bg">
        <input type="checkbox" class="oa-checkbox" checked={liteTools.textBackground} onchange={(e) => liteTools.applyTextBackground(e.currentTarget.checked)} />
        <span>{t("Label background")}</span>
      </label>
    </section>
  {/if}
</div>

<style>
  .markup {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .lt-tools {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
  .selected {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    border: var(--border-width) solid var(--border-hairline);
  }
  .custom {
    display: grid;
    place-items: center;
    color: var(--text-muted);
    background: var(--surface-card);
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  .custom[aria-pressed="true"] {
    background: var(--c);
    color: transparent;
  }
  .bar {
    display: block;
    width: 22px;
    border-radius: 4px;
    background: currentColor;
  }
  .bg {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--type-ui);
    font-weight: var(--weight-regular);
    color: var(--text-body);
    min-height: 32px;
    cursor: pointer;
  }
</style>
