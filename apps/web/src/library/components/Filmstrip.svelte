<script lang="ts">
  // A horizontal strip of thumbnails under the loupe, compare and survey
  // views. Virtualised like the grid.
  import { t } from "../../lib/i18n";
  import { library } from "../store.svelte";
  import Marks from "./Marks.svelte";

  const W = 84;
  const GAP = 4;
  let strip: HTMLDivElement;
  let width = $state(0);
  let scrollLeft = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollLeft / (W + GAP)) - 4));
  const last = $derived(Math.min(library.view.length, Math.ceil((scrollLeft + width) / (W + GAP)) + 4));
  const shown = $derived(library.view.slice(first, last));

  $effect(() => {
    library.setVisible(library.view.slice(first, Math.min(library.view.length, last + 8)));
  });

  $effect(() => {
    const i = library.focusIndex;
    if (i < 0 || !strip) return;
    const x = i * (W + GAP);
    if (x < strip.scrollLeft || x + W > strip.scrollLeft + width) strip.scrollTo({ left: x - width / 2 + W / 2 });
  });
</script>

<div class="strip lib-scroll" bind:this={strip} bind:clientWidth={width} onscroll={() => (scrollLeft = strip.scrollLeft)} aria-label={t("Filmstrip")} data-testid="lib-filmstrip">
  <div class="track" style:width="{library.view.length * (W + GAP)}px">
    {#each shown as it, k (it.path)}
      {@const i = first + k}
      {@const m = library.marksOf(it.path)}
      <button
        type="button"
        class="thumb"
        class:focused={library.focus === it.path}
        class:selected={library.selected.has(it.path)}
        class:rejected={m.flag === -1}
        style:left="{i * (W + GAP)}px"
        onclick={(e) => library.select(it.path, e.shiftKey ? "range" : e.metaKey || e.ctrlKey ? "toggle" : "replace")}
        aria-label={it.name}
      >
        {#if library.thumbs.get(it.path)}<img src={library.thumbs.get(it.path)} alt="" draggable="false" />{/if}
        <span class="marks"><Marks marks={m} size={8} /></span>
      </button>
    {/each}
  </div>
</div>

<style>
  .strip {
    flex: 0 0 76px;
    overflow-x: auto;
    overflow-y: hidden;
    border-top: var(--border-width) solid var(--border-hairline);
    background: var(--bg-subtle);
  }
  .track {
    position: relative;
    height: 100%;
    margin: 0 8px;
  }
  .thumb {
    position: absolute;
    top: 6px;
    width: 84px;
    height: 62px;
    padding: 2px;
    display: grid;
    place-items: center;
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-xs);
    background: var(--bg-sunken);
    cursor: default;
  }
  .thumb.selected {
    border-color: var(--border-strong);
  }
  .thumb.focused {
    border-color: var(--border-focus);
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .rejected img {
    opacity: 0.35;
  }
  .marks {
    position: absolute;
    left: 3px;
    bottom: 2px;
  }
</style>
