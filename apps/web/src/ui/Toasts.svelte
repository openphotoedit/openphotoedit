<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { editor } from "../lib/editor.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each editor.toasts as t (t.id)}
    <div class="toast" class:error={t.kind === "error"} role={t.kind === "error" ? "alert" : "status"}>
      <span>{t.text}</span>
      {#if t.action}
        <button class="oa-btn oa-btn--ghost" onclick={() => { t.action!.run(); editor.dismiss(t.id); }}>{t.action.label}</button>
      {/if}
      <button class="oa-icon-btn oa-icon-btn--sm" aria-label="Dismiss" onclick={() => editor.dismiss(t.id)}><X size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: calc(var(--space-6) + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    z-index: 100;
    pointer-events: none;
    max-width: min(560px, calc(100vw - 32px));
  }
  /* Phones: clear Lite's bottom tab bar and sheet handle. */
  @media (max-width: 760px) {
    .toasts {
      bottom: calc(88px + env(safe-area-inset-bottom));
    }
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--surface-inverse);
    color: var(--text-inverse);
    font: var(--type-ui);
    box-shadow: var(--shadow-lg, 0 8px 24px rgb(0 0 0 / 0.2));
  }
  .toast.error {
    background: var(--danger-bg);
    color: var(--danger-fg);
    border: 1px solid var(--danger-fg);
  }
  .toast button {
    color: inherit;
  }
</style>
