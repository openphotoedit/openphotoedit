<script lang="ts">
  import { editor } from "../lib/editor.svelte";
</script>

{#if editor.busy}
  <div class="busy" role="status" aria-live="polite" data-testid="busy">
    <div class="card">
      <span class="label">{editor.busy.label}</span>
      <div class="bar" class:indeterminate={editor.busy.fraction == null}>
        <span style:width={editor.busy.fraction != null ? `${Math.round(editor.busy.fraction * 100)}%` : undefined}></span>
      </div>
      {#if editor.busy.cancel}
        <button class="oa-btn oa-btn--secondary" onclick={() => editor.busy?.cancel?.()}>Cancel</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .busy {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, var(--bg-page) 40%, transparent);
    z-index: 90;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    align-items: stretch;
    min-width: 260px;
    padding: var(--space-5);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    border: 1px solid var(--border-hairline);
    box-shadow: var(--shadow-lg, 0 8px 24px rgb(0 0 0 / 0.2));
  }
  .label {
    font: var(--type-ui);
    color: var(--text-strong);
  }
  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--bg-sunken);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--text-strong);
    transition: width var(--duration-fast) linear;
  }
  .bar.indeterminate span {
    width: 30%;
    animation: slide 1.1s var(--ease-standard, ease) infinite;
  }
  @keyframes slide {
    from { transform: translateX(-100%); }
    to { transform: translateX(340%); }
  }
</style>
