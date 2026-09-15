<script lang="ts">
  // Image › Canvas Size: new width and height (absolute or relative), and
  // which edge stays put, chosen on a 3×3 anchor grid.
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import ArrowUpLeft from "@lucide/svelte/icons/arrow-up-left";
  import ArrowUpRight from "@lucide/svelte/icons/arrow-up-right";
  import ArrowDownLeft from "@lucide/svelte/icons/arrow-down-left";
  import ArrowDownRight from "@lucide/svelte/icons/arrow-down-right";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Dialog from "../../ui/Dialog.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import Select from "../../ui/Select.svelte";
  import { run } from "../engine.svelte";
  import { pro } from "../state.svelte";

  const s = editor.summary!;
  const ANCHORS = ["top-left", "top", "top-right", "left", "center", "right", "bottom-left", "bottom", "bottom-right"] as const;
  const NAMES = [t("Top left"), t("Top"), t("Top right"), t("Left"), t("Centre"), t("Right"), t("Bottom left"), t("Bottom"), t("Bottom right")];
  let anchor = $state(4);
  let unit = $state<"px" | "percent">("px");
  let relative = $state(false);
  let w = $state(s.width);
  let h = $state(s.height);

  const shown = (v: number, base: number) => {
    const n = relative ? v - base : v;
    return unit === "px" ? n : Math.round((n / base) * 1000) / 10;
  };
  const parse = (v: number, base: number) => Math.max(1, Math.round((unit === "px" ? v : (v / 100) * base) + (relative ? base : 0)));

  // Arrows point away from the anchor, as Photoshop draws them.
  const ICONS = [ArrowUpLeft, ArrowUp, ArrowUpRight, ArrowLeft, null, ArrowRight, ArrowDownLeft, ArrowDown, ArrowDownRight];
  function iconFor(cell: number) {
    const ar = Math.floor(anchor / 3);
    const ac = anchor % 3;
    const r = Math.floor(cell / 3);
    const c = cell % 3;
    const dr = r - ar;
    const dc = c - ac;
    if (Math.abs(dr) > 1 || Math.abs(dc) > 1) return null;
    if (dr === 0 && dc === 0) return "anchor";
    return ICONS[(dr + 1) * 3 + (dc + 1)];
  }

  async function ok() {
    pro.close();
    if (w !== s.width || h !== s.height) await run({ op: "image.canvas-size", width: w, height: h, anchor: ANCHORS[anchor] }, undefined, t("Canvas Size"));
  }
</script>

<Dialog title={t("Canvas Size")} width={360} testid="canvas-size-dialog" onclose={() => pro.close()} onsubmit={ok}>
  <div class="ops-stack">
    <p class="summary">{t("Current size")}: <strong>{s.width} × {s.height} px</strong></p>
    <hr class="ops-hr" />
    <div class="ops-row">
      <span class="ops-label lbl">{t("Width")}</span>
      <NumberField value={shown(w, s.width)} min={relative ? -300000 : 1} max={300000} step={unit === "px" ? 1 : 0.1} width={80} ariaLabel={t("Width")} onchange={(v) => (w = parse(v, s.width))} />
      <span class="grow"></span>
      <Select
        ariaLabel={t("Units")}
        value={unit}
        width={92}
        options={[
          { value: "px", label: t("Pixels") },
          { value: "percent", label: t("Percent") },
        ]}
        onchange={(u) => (unit = u)}
      />
    </div>
    <div class="ops-row">
      <span class="ops-label lbl">{t("Height")}</span>
      <NumberField value={shown(h, s.height)} min={relative ? -300000 : 1} max={300000} step={unit === "px" ? 1 : 0.1} width={80} ariaLabel={t("Height")} onchange={(v) => (h = parse(v, s.height))} />
    </div>
    <label class="ops-check"><input type="checkbox" bind:checked={relative} />{t("Relative")}</label>
    <div class="ops-row anchor-row">
      <span class="ops-label lbl">{t("Anchor")}</span>
      <div class="anchor" role="radiogroup" aria-label={t("Anchor")}>
        {#each NAMES as name, i (i)}
          {@const icon = iconFor(i)}
          <button type="button" role="radio" aria-checked={anchor === i} aria-label={name} class:on={anchor === i} onclick={() => (anchor = i)}>
            {#if icon === "anchor"}<span class="dot"></span>{:else if icon}{@const Icon = icon}<Icon size={11} />{/if}
          </button>
        {/each}
      </div>
    </div>
    <p class="summary">{t("New size")}: <strong>{w} × {h} px</strong></p>
  </div>
  {#snippet footer()}
    <button type="button" class="oa-btn oa-btn--secondary" onclick={() => pro.close()}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={ok}>{t("OK")}</button>
  {/snippet}
</Dialog>

<style>
  .summary {
    margin: 0;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  strong {
    font-weight: var(--weight-medium);
    color: var(--text-strong);
  }
  .lbl {
    width: 72px;
  }
  .grow {
    flex: 1;
  }
  .anchor-row {
    align-items: flex-start;
  }
  .anchor {
    display: grid;
    grid-template-columns: repeat(3, 24px);
    gap: 2px;
  }
  .anchor button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border-hairline);
    border-radius: 2px;
    background: var(--bg-sunken);
    color: var(--text-muted);
    cursor: default;
  }
  .anchor button.on {
    border-color: var(--text-strong);
  }
  .dot {
    width: 8px;
    height: 8px;
    background: var(--text-strong);
    border-radius: 1px;
  }
</style>
