<script lang="ts">
  // Every change, in order. Tap one to go back to just after it; the later
  // steps stay listed (dimmed) until something new is done.
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import ImageIcon from "@lucide/svelte/icons/image";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { lite } from "./lite.svelte";

  const undo = $derived(editor.summary?.history.undo ?? []);
  const redo = $derived.by(() => {
    const r = editor.summary?.history.redo ?? [];
    // Phantom entries sit at the far end of the redo list.
    return r.slice(0, Math.max(0, r.length - lite.phantomRedo));
  });

  interface Row {
    /** History position after this row's last entry. */
    index: number;
    label: string;
    future: boolean;
    group: number;
  }

  // Consecutive entries from one action (a Look is a layer plus its tint)
  // show as one row.
  const rows = $derived.by(() => {
    void lite.notesTick;
    const all = [...undo, ...redo];
    const out: Row[] = [];
    all.forEach((engine, i) => {
      const label = lite.stepLabel(i, engine);
      const group = lite.stepGroup(i, engine);
      const future = i >= undo.length;
      const last = out[out.length - 1];
      if (last && group > 0 && last.group === group && last.future === future) last.index = i + 1;
      else out.push({ index: i + 1, label, future, group });
    });
    return out;
  });
</script>

<div class="steps" data-testid="steps">
  <ol class="list">
    <li>
      <button class="row" class:current={undo.length === 0} onclick={() => lite.goTo(0)} data-testid="step-0">
        <span class="dot"><ImageIcon size={12} /></span>
        <span class="text">{t("Original")}</span>
      </button>
    </li>
    {#each rows as r, n (r.index)}
      <li>
        <button class="row" class:current={!r.future && r.index === undo.length} class:future={r.future} onclick={() => lite.goTo(r.index)} data-testid="step-{n + 1}">
          <span class="dot">{n + 1}</span>
          <span class="text">{r.label}</span>
        </button>
      </li>
    {/each}
  </ol>
  {#if !undo.length && !redo.length}
    <p class="lt-hint empty">{t("Nothing yet. Every change you make shows up here, and you can go back to any of them.")}</p>
  {/if}
  <button class="oa-btn oa-btn--secondary oa-btn--md over" disabled={!undo.length} data-testid="start-over" onclick={() => lite.goTo(0)}>
    <RotateCcw size={16} />
    {t("Start over")}
  </button>
</div>

<style>
  .steps {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .list::before {
    content: "";
    position: absolute;
    left: 19px;
    top: 18px;
    bottom: 18px;
    width: 1px;
    background: var(--border-hairline);
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: var(--touch-min);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: var(--transition-control);
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .dot {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-strong);
    font: var(--type-mono);
    font-size: var(--text-2xs);
    color: var(--text-muted);
  }
  .text {
    font: var(--type-ui);
    font-weight: var(--weight-regular);
    color: var(--text-body);
  }
  .current {
    background: var(--bg-subtle);
  }
  .current .dot {
    background: var(--text-strong);
    border-color: var(--text-strong);
    color: var(--bg-page);
  }
  .current .text {
    color: var(--text-strong);
    font-weight: var(--weight-medium);
  }
  .future .text,
  .future .dot {
    opacity: 0.5;
  }
  .future .text {
    text-decoration: line-through;
    text-decoration-color: var(--border-strong);
  }
  .empty {
    padding: 0 var(--space-2);
  }
  .over {
    align-self: flex-start;
  }
</style>
