<script lang="ts">
  // Culling badges for a photo: soft, eyes closed, exposure, similar group, best.
  import Star from "@lucide/svelte/icons/star";
  import Focus from "@lucide/svelte/icons/focus";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Sun from "@lucide/svelte/icons/sun";
  import Moon from "@lucide/svelte/icons/moon";
  import Copy from "@lucide/svelte/icons/copy";
  import { t } from "../../lib/i18n";
  import { tooltip } from "../../ui/tooltip";
  import type { CullResult } from "../types";

  let { result, compact = false }: { result: CullResult | undefined; compact?: boolean } = $props();
</script>

{#if result && result.badges.length}
  <span class="badges" class:compact data-testid="lib-badges" data-badges={result.badges.join(" ")}>
    {#if result.badges.includes("best")}
      <span class="b best" use:tooltip={t("Best of {n} similar shots", { n: result.groupSize ?? 2 })}><Star size={10} fill="currentColor" />{#if !compact}{t("Best")}{/if}</span>
    {/if}
    {#if result.badges.includes("blurry")}
      <span class="b warn" use:tooltip={t("Looks out of focus or shaken")}><Focus size={10} />{#if !compact}{t("Soft")}{/if}</span>
    {/if}
    {#if result.badges.includes("eyes-closed")}
      <span class="b warn" use:tooltip={t("Eyes may be closed")}><EyeOff size={10} />{#if !compact}{t("Eyes")}{/if}</span>
    {/if}
    {#if result.badges.includes("over")}
      <span class="b warn" use:tooltip={t("Overexposed: highlights are clipped")}><Sun size={10} />{#if !compact}{t("Bright")}{/if}</span>
    {/if}
    {#if result.badges.includes("under")}
      <span class="b warn" use:tooltip={t("Underexposed: shadows are crushed")}><Moon size={10} />{#if !compact}{t("Dark")}{/if}</span>
    {/if}
    {#if result.badges.includes("duplicate")}
      <span class="b" use:tooltip={t("One of {n} similar shots", { n: result.groupSize ?? 2 })}><Copy size={10} />{result.groupSize}</span>
    {/if}
  </span>
{/if}

<style>
  .badges {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 3px;
    pointer-events: auto;
  }
  .b {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 18px;
    padding: 0 5px;
    border-radius: var(--radius-xs);
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-sans);
    background: color-mix(in oklab, var(--surface-inverse) 72%, transparent);
    color: var(--text-inverse);
    backdrop-filter: blur(4px);
    white-space: nowrap;
  }
  .b.warn {
    background: var(--warning-bg);
    color: var(--warning-fg);
  }
  .b.best {
    background: var(--success-bg);
    color: var(--success-fg);
  }
</style>
