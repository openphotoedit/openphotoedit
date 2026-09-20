<script lang="ts">
  // Two photos side by side with synced zoom and pan. The focused photo is
  // the "select"; the other is the "candidate". Clicking a side focuses it;
  // arrow keys (handled by LibraryView) step the candidate.
  import Link from "@lucide/svelte/icons/link";
  import Unlink from "@lucide/svelte/icons/unlink";
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import { t } from "../../lib/i18n";
  import IconButton from "../../ui/IconButton.svelte";
  import { library } from "../store.svelte";
  import type { LibraryItem } from "../types";
  import Marks from "./Marks.svelte";
  import Badges from "./Badges.svelte";
  import Viewer, { type ViewerState } from "./Viewer.svelte";

  let { pair = $bindable<[string | null, string | null]>([null, null]) }: { pair?: [string | null, string | null] } = $props();

  let synced = $state(true);
  let viewA = $state<ViewerState>({ zoom: "fit", cx: 0.5, cy: 0.5 });
  let viewB = $state<ViewerState>({ zoom: "fit", cx: 0.5, cy: 0.5 });

  const byPath = (p: string | null): LibraryItem | null => (p ? (library.view.find((i) => i.path === p) ?? library.items.find((i) => i.path === p) ?? null) : null);
  const a = $derived(byPath(pair[0]));
  const b = $derived(byPath(pair[1]));

  function setA(v: ViewerState) {
    viewA = v;
    if (synced) viewB = { ...v };
  }
  function setB(v: ViewerState) {
    viewB = v;
    if (synced) viewA = { ...v };
  }
</script>

<div class="compare" data-testid="lib-compare">
  {#each [{ it: a, side: 0 }, { it: b, side: 1 }] as { it, side } (side)}
    <div class="pane">
      <div class="view">
        {#if side === 0}
          <Viewer item={it} bind:view={() => viewA, setA} active={!!it && library.focus === it.path} label={t("Select")} onactivate={() => it && library.select(it.path)} testid="lib-compare-a" />
        {:else}
          <Viewer item={it} bind:view={() => viewB, setB} active={!!it && library.focus === it.path} label={t("Candidate")} onactivate={() => it && library.select(it.path)} testid="lib-compare-b" />
        {/if}
      </div>
      <div class="caption">
        {#if it}
          <span class="name">{it.name}</span>
          <Badges result={library.cull.get(it.path)} />
          <span class="grow"></span>
          <Marks marks={library.marksOf(it.path)} size={11} />
        {/if}
      </div>
    </div>
  {/each}
  <div class="tools">
    <IconButton label={synced ? t("Unlink zoom and pan") : t("Link zoom and pan")} pressed={synced} onclick={() => (synced = !synced)} testid="lib-compare-sync">
      {#if synced}<Link size={14} />{:else}<Unlink size={14} />{/if}
    </IconButton>
    <IconButton label={t("Swap sides")} onclick={() => (pair = [pair[1], pair[0]])}><ArrowLeftRight size={14} /></IconButton>
  </div>
</div>

<style>
  .compare {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
    background: var(--border-hairline);
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--bg-sunken);
  }
  .view {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .caption {
    flex: 0 0 32px;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    background: var(--bg-subtle);
    border-top: var(--border-width) solid var(--border-hairline);
    min-width: 0;
  }
  .name {
    font: var(--type-caption);
    color: var(--text-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grow {
    flex: 1;
  }
  .tools {
    position: absolute;
    left: 50%;
    top: 8px;
    transform: translateX(-50%);
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    border: var(--border-width) solid var(--border-hairline);
    box-shadow: var(--shadow-md);
  }
</style>
