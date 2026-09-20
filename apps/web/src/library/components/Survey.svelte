<script lang="ts">
  // N photos at once, each fitted. Click a tile to focus it (keys then act on
  // it); the close button drops a tile from the survey without changing marks.
  import X from "@lucide/svelte/icons/x";
  import { t } from "../../lib/i18n";
  import { library } from "../store.svelte";
  import Badges from "./Badges.svelte";
  import Marks from "./Marks.svelte";
  import Viewer from "./Viewer.svelte";

  let { paths = $bindable<string[]>([]) }: { paths?: string[] } = $props();

  const items = $derived(paths.map((p) => library.items.find((i) => i.path === p)).filter((i) => !!i));
  const cols = $derived(Math.ceil(Math.sqrt(Math.max(1, items.length) * 1.5)));
</script>

<div class="survey" data-testid="lib-survey" style:grid-template-columns="repeat({cols}, 1fr)">
  {#each items as it (it.path)}
    {@const m = library.marksOf(it.path)}
    <div class="tile" class:focused={library.focus === it.path} class:rejected={m.flag === -1} data-testid="lib-survey-tile" data-path={it.path}>
      <div class="view">
        <Viewer item={it} active={library.focus === it.path} onactivate={() => library.select(it.path)} testid="lib-survey-viewer" />
      </div>
      <div class="caption">
        <span class="name">{it.name}</span>
        <Badges result={library.cull.get(it.path)} compact />
        <span class="grow"></span>
        <Marks marks={m} size={10} />
        <button type="button" class="close" aria-label={t("Remove from survey")} onclick={() => (paths = paths.filter((p) => p !== it.path))}><X size={12} /></button>
      </div>
    </div>
  {:else}
    <p class="oa-empty">{t("Select two or more photos, then press N to survey them.")}</p>
  {/each}
</div>

<style>
  .survey {
    position: absolute;
    inset: 0;
    display: grid;
    grid-auto-rows: 1fr;
    gap: 8px;
    padding: 8px;
    background: var(--bg-sunken);
  }
  .tile {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--surface-card);
  }
  .tile.focused {
    border-color: var(--border-focus);
  }
  .tile.rejected .view {
    opacity: 0.4;
  }
  .view {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .caption {
    flex: 0 0 28px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 4px 0 8px;
    min-width: 0;
  }
  .name {
    font: var(--type-caption);
    font-size: var(--text-2xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grow {
    flex: 1;
  }
  .close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .close:hover {
    background: var(--surface-hover);
    color: var(--text-strong);
  }
</style>
