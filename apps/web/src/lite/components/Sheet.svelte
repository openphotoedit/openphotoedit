<script lang="ts">
  // A modal surface: a bottom sheet on a phone, a centred dialog on a
  // desktop. Escape and the scrim close it.
  import type { Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";
  import { t } from "../../lib/i18n";

  let {
    title,
    open,
    onclose,
    width = 460,
    testid,
    children,
    footer,
  }: {
    title: string;
    open: boolean;
    onclose: () => void;
    width?: number;
    testid?: string;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let panel = $state<HTMLDivElement>();

  $effect(() => {
    if (open && panel) {
      const first = panel.querySelector<HTMLElement>("[data-autofocus]");
      (first ?? panel).focus();
    }
  });
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
    <div
      class="sheet"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      data-testid={testid}
      style="--w: {width}px"
      bind:this={panel}
      onkeydown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onclose();
        }
      }}
    >
      <span class="grip" aria-hidden="true"></span>
      <header>
        <h2>{title}</h2>
        <button class="oa-icon-btn" aria-label={t("Close")} onclick={onclose}><X size={18} /></button>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}
        <footer>{@render footer()}</footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: var(--space-6);
    background: var(--scrim);
    animation: fade var(--duration-base) var(--ease-standard);
  }
  .sheet {
    width: 100%;
    max-width: var(--w);
    max-height: calc(100dvh - var(--space-6) * 2);
    display: flex;
    flex-direction: column;
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-xl);
    overflow: hidden;
    outline: none;
    animation: rise var(--duration-slow) var(--ease-out);
  }
  .grip {
    display: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-3) var(--space-2) var(--space-5);
  }
  h2 {
    margin: 0;
    font: var(--weight-medium) var(--text-lg) / 1.25 var(--font-sans);
    letter-spacing: var(--tracking-heading);
    color: var(--text-strong);
  }
  .body {
    padding: var(--space-2) var(--space-5) var(--space-5);
    overflow-y: auto;
    min-height: 0;
  }
  footer {
    display: flex;
    gap: var(--space-2);
    justify-content: flex-end;
    padding: var(--space-3) var(--space-5);
    border-top: var(--border-width) solid var(--border-hairline);
    background: var(--bg-subtle);
  }
  @media (max-width: 899px) {
    .scrim {
      place-items: end stretch;
      padding: 0;
    }
    .sheet {
      max-width: none;
      max-height: 88dvh;
      border-radius: var(--sheet-radius) var(--sheet-radius) 0 0;
      border-bottom: 0;
      padding-bottom: var(--safe-bottom);
      animation: up var(--duration-slow) var(--ease-out);
    }
    .grip {
      display: block;
      width: 36px;
      height: 4px;
      margin: var(--space-2) auto 0;
      border-radius: var(--radius-full);
      background: var(--border-strong);
    }
    header {
      padding-top: var(--space-2);
    }
    .body {
      padding: var(--space-2) var(--space-4) var(--space-4);
    }
    footer {
      padding: var(--space-3) var(--space-4);
    }
  }
  @keyframes fade {
    from { opacity: 0; }
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px); }
  }
  @keyframes up {
    from { transform: translateY(24px); opacity: 0.6; }
  }
</style>
