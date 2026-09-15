<script lang="ts">
  // Heavier fixes that run a model on this device. Each says in one line
  // what it does; progress (including the one-time model download) shows
  // on the card that started it.
  import Scaling from "@lucide/svelte/icons/scaling";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
  import ScanFace from "@lucide/svelte/icons/scan-face";
  import Palette from "@lucide/svelte/icons/palette";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import type { Quality } from "../../lib/ai";
  import { lite } from "../lite.svelte";
  import { cleanJpeg, colorize, denoise, restoreFaces, upscale } from "../ops";

  let quality = $state<Quality>("fast");

  const ITEMS = [
    { key: "upscale", title: "Upscale", text: "Makes the photo two or four times larger with sharper detail.", icon: Scaling },
    { key: "denoise", title: "Denoise", text: "Smooths the speckle from low light and phone cameras.", icon: Sparkles },
    { key: "jpeg", title: "Clean up JPEG", text: "Removes blocky compression marks from saved and shared photos.", icon: Grid2x2 },
    { key: "faces", title: "Restore faces", text: "Brings back detail in small or blurry faces.", icon: ScanFace },
    { key: "colorize", title: "Colorize", text: "Adds natural color to a black and white photo.", icon: Palette },
  ] as const;

  function isWorking(key: string) {
    return lite.working === key || (key === "upscale" && !!lite.working?.startsWith("upscale"));
  }
  function isSoon(key: string) {
    return key === "upscale" ? !!(lite.unavailable["upscale-2"] || lite.unavailable["upscale-4"]) : !!lite.unavailable[key];
  }
</script>

<div class="enhance">
  <ul class="list">
    {#each ITEMS as item (item.key)}
      {@const Icon = item.icon}
      {@const working = isWorking(item.key)}
      {@const soon = isSoon(item.key)}
      <li class="lt-card" class:soon data-testid="enhance-{item.key}">
        <span class="lt-card__icon">{#if working}<LoaderCircle size={18} class="lt-spin" />{:else}<Icon size={18} />{/if}</span>
        <span class="copy">
          <span class="lt-card__title">
            {t(item.title)}
            {#if soon}<span class="oa-badge oa-badge--sm">{t("Coming soon")}</span>{/if}
          </span>
          <span class="lt-card__text">{working && editor.busy ? editor.busy.label : t(item.text)}</span>
          {#if working && editor.busy}
            <span class="lt-progress" class:lt-progress--indeterminate={editor.busy.fraction == null}>
              <span style:width={editor.busy.fraction != null ? `${Math.round(editor.busy.fraction * 100)}%` : undefined}></span>
            </span>
          {/if}
        </span>
        <span class="go">
          {#if item.key === "upscale"}
            <button class="oa-btn oa-btn--secondary" disabled={!editor.hasDocument || !!lite.working || soon} onclick={() => upscale(2, quality)}>2×</button>
            <button class="oa-btn oa-btn--secondary" disabled={!editor.hasDocument || !!lite.working || soon} onclick={() => upscale(4, quality)}>4×</button>
          {:else}
            <button
              class="oa-btn oa-btn--secondary"
              disabled={!editor.hasDocument || !!lite.working || soon}
              onclick={() => (item.key === "denoise" ? denoise() : item.key === "jpeg" ? cleanJpeg() : item.key === "faces" ? restoreFaces() : colorize())}
            >
              {t("Apply")}
            </button>
          {/if}
        </span>
      </li>
    {/each}
  </ul>
  <div class="quality">
    <span class="lt-card__title">{t("Quality")}</span>
    <div class="lt-seg" role="group" aria-label={t("Quality")}>
      <button aria-pressed={quality === "fast"} onclick={() => (quality = "fast")}>{t("Fast")}</button>
      <button aria-pressed={quality === "best"} onclick={() => (quality = "best")}>{t("Best")}</button>
    </div>
  </div>
  <p class="note"><ShieldCheck size={14} />{t("Models download once, then run on this device. Your photo is never uploaded.")}</p>
</div>

<style>
  .enhance {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .copy .lt-progress {
    margin-top: var(--space-2);
  }
  .go {
    display: flex;
    gap: var(--space-1);
  }
  .soon .lt-card__icon {
    color: var(--text-faint);
  }
  .quality {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-3);
    align-items: center;
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    font: var(--type-caption);
    color: var(--text-faint);
  }
  .note :global(svg) {
    flex: 0 0 auto;
    margin-top: 1px;
  }
</style>
