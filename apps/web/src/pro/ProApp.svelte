<script lang="ts">
  // Placeholder shell: replaced by the full Pro profile.
  import Canvas from "../canvas/Canvas.svelte";
  import BrandMark from "../ui/BrandMark.svelte";
  import { editor } from "../lib/editor.svelte";
  import { openFile, pickFiles } from "../lib/io";

  async function open() {
    const [f] = await pickFiles();
    if (f) await openFile(f);
  }
</script>

<header class="bar">
  <BrandMark />
  <button class="oa-btn oa-btn--secondary" onclick={open}>Open</button>
  <span class="grow"></span>
  <button class="oa-btn oa-btn--ghost" onclick={() => editor.setProfile("lite")}>Simple mode</button>
</header>
<div class="main">
  <div class="stage"><Canvas /></div>
  <aside class="side">
    {#each [...(editor.summary?.layers ?? [])].reverse() as l (l.id)}
      <div class="layer" class:active={l.id === editor.summary?.active}>{l.name}</div>
    {/each}
  </aside>
</div>

<style>
  .bar { display: flex; align-items: center; gap: var(--space-2); height: 44px; padding: 0 var(--space-3); border-bottom: 1px solid var(--border-hairline); }
  .grow { flex: 1; }
  .main { flex: 1; display: flex; min-height: 0; }
  .stage { flex: 1; min-width: 0; }
  .side { width: 240px; border-left: 1px solid var(--border-hairline); }
  .layer { padding: 8px 12px; font: var(--type-ui); }
  .layer.active { background: var(--surface-selected); }
</style>
