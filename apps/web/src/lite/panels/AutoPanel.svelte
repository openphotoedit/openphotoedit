<script lang="ts">
  // Auto with alternatives: four engine-rendered results to choose from,
  // rather than one opaque button. The choice becomes the "Adjustments"
  // layer, which the Adjust sliders then fine-tune.
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { DEVELOP_DEFAULT, type Develop } from "../../engine/types";
  import Tile from "../components/Tile.svelte";
  import { adjustments, ADJUSTMENTS, lite } from "../lite.svelte";
  import { AUTO_STYLES, autoAdjustment, autoDevelop, applyAuto } from "../ops";
  import { docSignature, PreviewSet, currentThumb, type Thumb } from "../previews.svelte";
  import type { AutoStyle } from "../develop";
  import { openCategory } from "../actions";

  const previews = new PreviewSet(256);
  let develops = $state<Partial<Record<AutoStyle, Develop>>>({});
  let original = $state<Thumb | null>(null);
  let applying = $state<AutoStyle | null>(null);

  const signature = $derived(editor.hasDocument && editor.summary ? docSignature([ADJUSTMENTS]) : "");

  const same = (a: Develop, b: Develop) => (Object.keys(DEVELOP_DEFAULT) as (keyof Develop)[]).every((k) => Math.abs(a[k] - b[k]) < 0.011);

  const chosen = $derived.by(() => {
    const cur = adjustments.stored;
    for (const s of AUTO_STYLES) {
      const d = develops[s.id];
      if (d && same(autoAdjustment(d), cur)) return s.id;
    }
    return null;
  });

  $effect(() => {
    const sig = signature;
    if (!sig) return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      const next: Partial<Record<AutoStyle, Develop>> = {};
      for (const s of AUTO_STYLES) next[s.id] = await lite.lock.run(() => autoDevelop(s.id));
      if (cancelled) return;
      develops = next;
      const cur = adjustments.layer;
      await previews.ensure(
        sig,
        AUTO_STYLES.map((s) => ({
          key: s.id,
          commands: () => {
            const adjustment = { kind: "develop", ...autoAdjustment(next[s.id]!) };
            return cur
              ? [{ op: "layer.set-adjustment", id: cur.id, adjustment }]
              : [{ op: "layer.add-adjustment", adjustment, name: ADJUSTMENTS, above: lite.anchorFor(ADJUSTMENTS) }];
          },
        })),
      );
      if (!cancelled && previews.blocked && !original) original = await currentThumb(256);
    }, 60);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => () => {
    previews.clear();
    if (original) URL.revokeObjectURL(original.url);
  });

  async function choose(style: AutoStyle) {
    if (applying) return;
    applying = style;
    try {
      await applyAuto(style);
    } finally {
      applying = null;
    }
  }
</script>

<div class="auto">
  <div class="grid">
    {#each AUTO_STYLES as s (s.id)}
      <Tile
        label={t(s.label)}
        thumb={previews.thumbs[s.id]}
        fallback={original?.url ?? null}
        loading={previews.loading}
        selected={chosen === s.id}
        testid="auto-{s.id}"
        onclick={() => choose(s.id)}
      />
    {/each}
  </div>
  {#if previews.blocked}
    <p class="lt-hint">{t("Previews will appear after your next edit.")}</p>
  {/if}
  <p class="lt-hint">{t("Each version is a starting point you can fine-tune in Adjust.")}</p>
  {#if chosen}
    <button class="oa-btn oa-btn--secondary oa-btn--md" onclick={() => openCategory("adjust")}>
      <SlidersHorizontal size={16} />
      {t("Fine-tune in Adjust")}
    </button>
  {/if}
</div>

<style>
  .auto {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4) var(--space-3);
  }
  @media (max-width: 899px) {
    .grid {
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: var(--space-2);
    }
  }
</style>
