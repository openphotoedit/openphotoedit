<script lang="ts">
  import { untrack } from "svelte";
  // Looks with a strength slider, vignette and grain, blur background, and
  // frames. Looks live on their own "Look" layer, so they sit on top of
  // whatever was done in Adjust and can be swapped without losing it.
  import Focus from "@lucide/svelte/icons/focus";
  import Frame from "@lucide/svelte/icons/frame";
  import RectangleHorizontal from "@lucide/svelte/icons/rectangle-horizontal";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Slider from "../components/Slider.svelte";
  import Tile from "../components/Tile.svelte";
  import { identifyLook, lookDevelop, lookTint, LOOKS, type Look } from "../develop";
  import { adjustments, lite, look, LOOK, LOOK_TINT } from "../lite.svelte";
  import { addBorder, addCaption, applyLook, blurBackground, currentTintLayer, lookPreviewCommands } from "../ops";
  import { docSignature, PreviewSet } from "../previews.svelte";

  const previews = new PreviewSet(224);
  const signature = $derived(editor.hasDocument && editor.summary ? docSignature([LOOK, LOOK_TINT]) : "");
  const identified = $derived(look.layer ? identifyLook(look.stored) : null);
  let dragStrength = $state<number | null>(null);
  const strength = $derived(dragStrength ?? Math.round((identified?.strength ?? 1) * 100));
  let applying = $state<string | null>(null);

  let borderSize = $state<"s" | "m" | "l">("m");
  let borderColor = $state("#ffffff");
  let caption = $state("");

  $effect(() => {
    const sig = signature;
    if (!sig) return;
    // Wait for a gesture to settle; previews hold the engine while they run.
    const timer = setTimeout(
      () =>
        void previews.ensure(sig, [
          { key: "none", commands: () => lookPreviewCommands(null) },
          ...LOOKS.map((l) => ({ key: l.id, commands: () => lookPreviewCommands(l, 1) })),
        ]),
      untrack(() => (previews.thumbs.none ? 450 : 60)),
    );
    return () => clearTimeout(timer);
  });

  $effect(() => () => previews.clear());

  async function choose(l: Look | null) {
    if (applying) return;
    applying = l?.id ?? "none";
    try {
      await applyLook(l, l && identified?.look.id === l.id ? identified.strength : 1);
    } finally {
      applying = null;
    }
  }

  function setStrength(v: number) {
    const cur = identified?.look;
    if (!cur) return;
    dragStrength = v;
    look.set(lookDevelop(cur, v / 100));
  }

  async function commitStrength() {
    const cur = identified?.look;
    const v = dragStrength;
    if (!cur || v == null) return;
    await look.commit();
    const tint = currentTintLayer();
    if (cur.tint && tint) {
      await lite.exec({ op: "layer.set-adjustment", id: tint.id, adjustment: lookTint(cur, v / 100) });
      await lite.exec({ op: "edit.seal" });
    }
    lite.note(t("{name} at {value}%", { name: t(cur.name), value: v }), /Develop|Color Balance|Adjustment/);
    dragStrength = null;
  }

  const fx = $derived(adjustments.values);
</script>

<div class="effects">
  <section class="lt-section">
    <div class="lt-section__head">
      <h3 class="lt-eyebrow">{t("Looks")}</h3>
      {#if previews.loading}<LoaderCircle size={14} class="lt-spin" aria-label={t("Rendering previews")} />{/if}
    </div>
    <div class={lite.phone ? "lt-tiles lt-tiles--row" : "lt-tiles"}>
      <Tile label={t("None")} thumb={previews.thumbs.none} loading={previews.loading} selected={!look.layer} testid="look-none" onclick={() => choose(null)} />
      {#each LOOKS as l (l.id)}
        <Tile label={t(l.name)} thumb={previews.thumbs[l.id]} loading={previews.loading} selected={identified?.look.id === l.id} testid="look-{l.id}" onclick={() => choose(l)} />
      {/each}
    </div>
    {#if previews.blocked}
      <p class="lt-hint">{t("Previews will appear after your next edit.")}</p>
    {/if}
    {#if identified}
      <Slider
        label={t("Strength")}
        value={strength}
        min={0}
        max={100}
        defaultValue={100}
        format={(v) => `${Math.round(v)}%`}
        testid="look-strength"
        oninput={setStrength}
        oncommit={commitStrength}
      />
    {/if}
  </section>

  <section class="lt-section">
    <h3 class="lt-eyebrow">{t("Finish")}</h3>
    <Slider
      label={t("Vignette")}
      value={fx.vignette}
      disabled={!editor.hasDocument}
      testid="slider-vignette"
      oninput={(v) => adjustments.set({ vignette: v })}
      oncommit={async () => {
        await adjustments.commit();
        lite.note(t("Vignette"), /Develop|Adjustment/);
      }}
    />
    <Slider
      label={t("Grain")}
      value={fx.grain}
      min={0}
      max={100}
      disabled={!editor.hasDocument}
      testid="slider-grain"
      oninput={(v) => adjustments.set({ grain: v })}
      oncommit={async () => {
        await adjustments.commit();
        lite.note(t("Grain"), /Develop|Adjustment/);
      }}
    />
    <button class="lt-card action" disabled={!editor.hasDocument || !!lite.working} onclick={() => blurBackground()}>
      <span class="lt-card__icon">{#if lite.working === "blur-bg"}<LoaderCircle size={18} class="lt-spin" />{:else}<Focus size={18} />{/if}</span>
      <span>
        <span class="lt-card__title">{t("Blur background")}{#if lite.unavailable["blur-bg"]}<span class="oa-badge oa-badge--sm">{t("Coming soon")}</span>{/if}</span>
        <span class="lt-card__text">{t("Finds the subject and softens everything behind it.")}</span>
      </span>
      <span></span>
    </button>
  </section>

  <section class="lt-section">
    <h3 class="lt-eyebrow">{t("Frames")}</h3>
    <div class="frame-row">
      <span class="lt-card__icon"><Frame size={18} /></span>
      <div class="frame-controls">
        <span class="lt-card__title">{t("Border")}</span>
        <div class="line">
          <div class="lt-seg sizes" role="group" aria-label={t("Border width")}>
            {#each [["s", "Thin"], ["m", "Medium"], ["l", "Thick"]] as [id, label] (id)}
              <button aria-pressed={borderSize === id} onclick={() => (borderSize = id as "s" | "m" | "l")}>{t(label)}</button>
            {/each}
          </div>
          <div class="colors">
            {#each ["#ffffff", "#111111"] as hex (hex)}
              <button class="lt-swatch small" style:background={hex} aria-label={hex === "#ffffff" ? t("White") : t("Black")} aria-pressed={borderColor === hex} onclick={() => (borderColor = hex)}></button>
            {/each}
          </div>
        </div>
      </div>
      <button class="oa-btn oa-btn--secondary oa-btn--md" disabled={!editor.hasDocument} data-testid="add-border" onclick={() => addBorder(borderSize === "s" ? 0.02 : borderSize === "m" ? 0.045 : 0.08, borderColor)}>{t("Add")}</button>
    </div>
    <div class="frame-row">
      <span class="lt-card__icon"><RectangleHorizontal size={18} /></span>
      <div class="frame-controls">
        <span class="lt-card__title">{t("Caption bar")}</span>
        <input class="oa-input" placeholder={t("Write a caption")} bind:value={caption} maxlength="80" />
      </div>
      <button class="oa-btn oa-btn--secondary oa-btn--md" disabled={!editor.hasDocument} onclick={async () => { await addCaption(caption, borderColor); caption = ""; }}>{t("Add")}</button>
    </div>
  </section>
</div>

<style>
  .effects {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .action {
    width: 100%;
    text-align: left;
    cursor: pointer;
  }
  .action:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .lt-card__title :global(.oa-badge) {
    margin-left: var(--space-2);
  }
  .frame-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: end;
    gap: var(--space-3);
  }
  .frame-row .lt-card__icon {
    align-self: start;
  }
  .frame-controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .line {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .sizes {
    flex: 1;
  }
  .colors {
    display: flex;
    gap: var(--space-2);
    padding-inline: var(--space-1);
  }
  .small {
    width: 26px;
    height: 26px;
  }
</style>
