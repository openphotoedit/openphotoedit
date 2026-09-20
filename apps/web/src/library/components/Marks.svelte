<script lang="ts">
  // A compact read-out of a photo's marks: stars, flag and colour label.
  // Interactive in the sidebar (`editable`), display-only in cells.
  import Star from "@lucide/svelte/icons/star";
  import Flag from "@lucide/svelte/icons/flag";
  import FlagOff from "@lucide/svelte/icons/flag-off";
  import { t } from "../../lib/i18n";
  import type { Marks } from "../types";

  let {
    marks,
    editable = false,
    size = 11,
    onrate,
  }: { marks: Marks; editable?: boolean; size?: number; onrate?: (r: number) => void } = $props();
</script>

<span class="marks" class:editable>
  {#if editable}
    <span class="stars" role="radiogroup" aria-label={t("Rating")}>
      {#each [1, 2, 3, 4, 5] as r (r)}
        <button
          type="button"
          role="radio"
          aria-checked={marks.rating === r}
          aria-label={t("{n} stars", { n: r })}
          class:on={marks.rating >= r}
          data-testid="lib-star-{r}"
          onclick={() => onrate?.(marks.rating === r ? 0 : r)}><Star size={size} fill={marks.rating >= r ? "currentColor" : "none"} /></button
        >
      {/each}
    </span>
  {:else if marks.rating > 0}
    <span class="stars" aria-label={t("{n} stars", { n: marks.rating })}>
      {#each Array(marks.rating) as _, i (i)}<Star size={size} fill="currentColor" />{/each}
    </span>
  {/if}
  {#if !editable}
    {#if marks.flag === 1}
      <span class="flag pick" aria-label={t("Picked")}><Flag size={size} fill="currentColor" /></span>
    {:else if marks.flag === -1}
      <span class="flag reject" aria-label={t("Rejected")}><FlagOff size={size} /></span>
    {/if}
    {#if marks.label}
      <span class="lib-dot" data-label={marks.label}></span>
    {/if}
  {/if}
</span>

<style>
  .marks {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--text-strong);
  }
  .stars {
    display: inline-flex;
    align-items: center;
    gap: 1px;
  }
  .editable .stars {
    gap: 0;
  }
  .editable button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .editable button:hover {
    background: var(--surface-hover);
  }
  .editable button.on {
    color: var(--text-strong);
  }
  .flag {
    display: inline-grid;
  }
  .flag.reject {
    color: var(--danger-fg);
  }
</style>
