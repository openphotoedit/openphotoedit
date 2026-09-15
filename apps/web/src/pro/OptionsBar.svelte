<script lang="ts">
  // The options bar under the menus: the active tool's own options component
  // from the tools workstream (`optionsFor(tool)`), or its hint until one exists.
  import type { Component } from "svelte";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { toolBridge, toolMeta } from "./tools.svelte";

  const meta = $derived(toolMeta(editor.tool));
  const Options = $derived.by(() => {
    const f = toolBridge.optionsFor;
    if (!f) return null;
    try {
      const c = f(editor.tool);
      return (typeof c === "function" ? c : null) as Component<Record<string, unknown>> | null;
    } catch {
      return null;
    }
  });
</script>

<div class="options" data-testid="options-bar">
  <div class="tool">
    {#key editor.tool}
      {@const Icon = meta.icon}
      <span class="icon"><Icon size={15} strokeWidth={1.75} /></span>
    {/key}
    <span class="name">{meta.label}</span>
  </div>
  <span class="rule" aria-hidden="true"></span>
  <div class="body">
    {#if Options}
      <Options ed={editor} editor={editor} profile="pro" />
    {:else if meta.hint}
      <span class="hint">{meta.hint}</span>
    {:else}
      <span class="hint">{t("No options for this tool.")}</span>
    {/if}
  </div>
</div>

<style>
  .options {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 36px;
    flex: 0 0 auto;
    padding: 0 var(--space-3);
    background: var(--bg-page);
    border-bottom: var(--border-width) solid var(--border-hairline);
    font: var(--type-caption);
    color: var(--text-body);
    min-width: 0;
  }
  .tool {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 0 0 auto;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 24px;
    border-radius: var(--radius-xs);
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .name {
    font: var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-strong);
    white-space: nowrap;
  }
  .rule {
    width: 1px;
    height: 20px;
    background: var(--border-hairline);
  }
  .body {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .hint {
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
