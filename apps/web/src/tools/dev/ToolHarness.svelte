<script lang="ts">
  import { onMount } from "svelte";
  import Canvas from "../../canvas/Canvas.svelte";
  import { editor } from "../../lib/editor.svelte";
  import { openFile } from "../../lib/io";
  import Toasts from "../../ui/Toasts.svelte";
  import { LITE_MARKUP_TOOLS, TOOL_GROUPS } from "../groups";
  import { optionsFor } from "../options";
  import { TOOLS } from "../registry";
  import { toolSettings } from "../settings.svelte";
  import { toolState } from "../state.svelte";

  const Options = $derived(optionsFor(editor.tool));

  onMount(() => {
    editor.init().catch((e) => editor.error(e));
    (window as unknown as Record<string, unknown>).__ops = { editor, TOOLS, toolSettings, toolState, openFile };
  });

  function onFile(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    if (f) void openFile(f);
  }
</script>

<div class="harness">
  <header class="bar">
    <input type="file" accept="image/*" data-testid="open" onchange={onFile} />
    <select class="oa-input" data-testid="tool" bind:value={editor.tool}>
      {#each TOOL_GROUPS as g (g.id)}
        <optgroup label={g.label}>
          {#each g.tools as tool (tool.id)}
            <option value={tool.id}>{tool.label}</option>
          {/each}
        </optgroup>
      {/each}
      <option value="transform">Free transform</option>
    </select>
    <span class="meta" data-testid="doc-info">
      {#if editor.summary && editor.hasDocument}
        {editor.summary.width} × {editor.summary.height} · {editor.summary.layers.length} layers · active {editor.active?.name ?? "none"}
        {editor.summary.selection ? ` · selection ${JSON.stringify(editor.summary.selection.bounds)}` : ""}
      {/if}
    </span>
    <span class="meta">Lite markup: {LITE_MARKUP_TOOLS.map((t) => t.id).join(", ")}</span>
  </header>
  <div class="options">
    {#if Options}
      <Options tool={editor.tool} />
    {/if}
  </div>
  <main class="stage">
    <Canvas />
  </main>
  <Toasts />
</div>

<style>
  .harness {
    height: 100%;
    display: grid;
    grid-template-rows: auto auto 1fr;
    background: var(--bg-page);
    color: var(--text-body);
  }
  .bar,
  .options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-2) var(--space-3);
    border-bottom: var(--border-width) solid var(--border-hairline);
    background: var(--surface-card);
  }
  .bar select {
    width: auto;
  }
  .meta {
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .stage {
    position: relative;
    min-height: 0;
  }
</style>
