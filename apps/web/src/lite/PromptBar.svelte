<script lang="ts">
  // "Describe an edit": a sentence becomes a short list of visible steps
  // the person reviews before anything changes. Suggestion chips double as
  // examples of what it understands.
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import X from "@lucide/svelte/icons/x";
  import Check from "@lucide/svelte/icons/check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { applyPlan, plan, SUGGESTIONS, type Step } from "./actions";
  import { lite } from "./lite.svelte";

  let { compact = false }: { compact?: boolean } = $props();

  let text = $state("");
  let focused = $state(false);
  let thinking = $state(false);
  let applying = $state(false);
  let steps = $state<Step[] | null>(null);
  let message = $state<string | null>(null);
  let asked = $state("");
  let input = $state<HTMLInputElement>();

  async function submit(q = text) {
    const query = q.trim();
    if (!query || thinking || !editor.hasDocument) return;
    thinking = true;
    message = null;
    asked = query;
    try {
      const r = await plan(query);
      steps = r.steps.length ? r.steps : null;
      message = r.unsupported ?? null;
    } finally {
      thinking = false;
    }
  }

  async function apply() {
    if (!steps?.length) return;
    applying = true;
    try {
      await applyPlan(steps);
      editor.toast(t("Done. Each change is in Steps if you want to undo it."), "success");
      reset();
    } finally {
      applying = false;
    }
  }

  function reset() {
    steps = null;
    message = null;
    text = "";
  }

  function removeStep(i: number) {
    if (!steps) return;
    const next = steps.filter((_, j) => j !== i);
    steps = next.length ? next : null;
  }

  // On a phone the chips appear while the field has focus, to save room.
  const showChips = $derived(!steps && !message && (!compact || focused));
</script>

<div class="prompt" class:compact class:focused data-testid="prompt">
  {#if steps || message}
    <div class="plan" role="region" aria-live="polite" aria-label={t("Planned edit")}>
      <div class="plan__head">
        <span class="plan__title">
          <Sparkles size={14} />
          {steps ? t("Here is the plan for “{text}”", { text: asked }) : t("“{text}”", { text: asked })}
        </span>
        <button class="oa-icon-btn oa-icon-btn--sm" aria-label={t("Cancel")} onclick={reset}><X size={14} /></button>
      </div>
      {#if steps}
        <ol class="steps">
          {#each steps as s, i (i)}
            <li class="step">
              <span class="num">{i + 1}</span>
              <span class="label">{s.label}</span>
              <button class="oa-icon-btn oa-icon-btn--sm" aria-label={t("Remove this step")} onclick={() => removeStep(i)}><X size={12} /></button>
            </li>
          {/each}
        </ol>
      {/if}
      {#if message}<p class="lt-hint">{message}</p>{/if}
      <div class="plan__actions">
        <button class="oa-btn oa-btn--ghost oa-btn--md" onclick={reset}>{t("Cancel")}</button>
        {#if steps}
          <button class="oa-btn oa-btn--primary oa-btn--md" data-testid="prompt-apply" disabled={applying || !!lite.working} onclick={apply}>
            {#if applying}<LoaderCircle size={16} class="lt-spin" />{:else}<Check size={16} />{/if}
            {t("Apply")}
          </button>
        {/if}
      </div>
    </div>
  {/if}

  <form class="field" onsubmit={(e) => { e.preventDefault(); submit(); }}>
    <Sparkles size={16} class="field__icon" />
    <input
      bind:this={input}
      bind:value={text}
      type="text"
      enterkeyhint="go"
      placeholder={t("Describe an edit…")}
      aria-label={t("Describe an edit")}
      disabled={!editor.hasDocument}
      data-testid="prompt-input"
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
      onkeydown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          if (steps || message) reset();
          else input?.blur();
        }
      }}
    />
    <button class="send" type="submit" aria-label={t("Plan this edit")} disabled={!text.trim() || thinking}>
      {#if thinking}<LoaderCircle size={16} class="lt-spin" />{:else}<ArrowUp size={16} />{/if}
    </button>
  </form>

  {#if showChips}
    <div class="lt-chips chips" class:lt-chips--scroll={compact} aria-label={t("Suggestions")}>
      {#each SUGGESTIONS as s (s.text)}
        <button
          class="lt-chip"
          disabled={!editor.hasDocument || thinking}
          onpointerdown={(e) => e.preventDefault()}
          onclick={() => {
            text = t(s.text);
            submit(t(s.text));
          }}>{t(s.text)}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .prompt {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
  }
  .field {
    position: relative;
    display: flex;
    align-items: center;
    height: var(--control-h-lg);
    border-radius: var(--radius-full);
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-strong);
    box-shadow: var(--shadow-md);
    transition: var(--transition-control);
  }
  .compact .field {
    box-shadow: none;
    background: var(--bg-subtle);
    border-color: var(--border-hairline);
  }
  .focused .field {
    border-color: var(--border-focus);
  }
  .field :global(.field__icon) {
    position: absolute;
    left: 14px;
    color: var(--text-muted);
    pointer-events: none;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 var(--space-2) 0 40px;
    border: 0;
    background: transparent;
    font: var(--type-body);
    font-size: 16px;
    color: var(--text-strong);
    outline: none;
  }
  input::placeholder {
    color: var(--text-faint);
  }
  input:focus-visible {
    box-shadow: none;
  }
  .send {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-right: 5px;
    border: 0;
    border-radius: 50%;
    background: var(--text-strong);
    color: var(--bg-page);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .send:disabled {
    background: var(--bg-sunken);
    color: var(--text-faint);
    cursor: default;
  }
  .chips {
    justify-content: center;
    gap: 6px;
  }
  .compact .chips {
    justify-content: flex-start;
    margin-inline: calc(var(--space-4) * -1);
    padding-inline: var(--space-4);
  }
  .chips .lt-chip {
    height: var(--control-h-sm);
    background: color-mix(in oklab, var(--surface-card) 92%, transparent);
    backdrop-filter: var(--blur-panel);
  }
  .plan {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    border: var(--border-width) solid var(--border-hairline);
    box-shadow: var(--shadow-lg);
    animation: rise var(--duration-base) var(--ease-out);
  }
  .compact .plan {
    box-shadow: none;
  }
  .plan__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .plan__title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--type-ui);
    color: var(--text-strong);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .step {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-h-md);
    padding: 0 var(--space-1) 0 var(--space-1);
    border-radius: var(--radius-full);
    background: var(--bg-subtle);
    border: var(--border-width) solid var(--border-hairline);
  }
  .num {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--text-strong);
    color: var(--bg-page);
    font: var(--type-mono);
    font-size: var(--text-2xs);
  }
  .label {
    font: var(--type-ui);
    color: var(--text-body);
  }
  .plan__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
