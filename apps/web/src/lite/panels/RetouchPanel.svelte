<script lang="ts">
  // Erase, heal, fix red eye, or start from the subject: once something is
  // selected, the actions that make sense for it appear.
  import Eraser from "@lucide/svelte/icons/eraser";
  import Bandage from "@lucide/svelte/icons/bandage";
  import Eye from "@lucide/svelte/icons/eye";
  import ScanFace from "@lucide/svelte/icons/scan-face";
  import ImageOff from "@lucide/svelte/icons/image-off";
  import Focus from "@lucide/svelte/icons/focus";
  import Sun from "@lucide/svelte/icons/sun";
  import X from "@lucide/svelte/icons/x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Slider from "../components/Slider.svelte";
  import { lite } from "../lite.svelte";
  import { blurBackground, brightenSubject, removeBackground, selectSubject } from "../ops";
  import { toolSettings } from "../../tools/settings.svelte";
  import { activatePixels, liteTools, type RetouchTool } from "../tools.svelte";

  const TOOLS: { id: RetouchTool; label: string; hint: string; icon: typeof Eraser }[] = [
    { id: "remove", label: "Erase", hint: "Paint over something to remove it.", icon: Eraser },
    { id: "spot-heal", label: "Spot heal", hint: "Tap a blemish or a speck of dust.", icon: Bandage },
    { id: "red-eye", label: "Red eye", hint: "Tap each red pupil, or drag a box around an eye.", icon: Eye },
  ];

  const current = $derived(TOOLS.find((x) => x.id === liteTools.retouch)!);

  function pick(id: RetouchTool) {
    liteTools.retouch = id;
    editor.tool = id;
    void activatePixels();
  }
  const hasSelection = $derived(!!editor.summary?.selection);
  const busyLabel = $derived(editor.busy?.label ?? null);
</script>

<div class="retouch">
  <section class="lt-section">
    <div class="lt-tools three">
      {#each TOOLS as tool (tool.id)}
        {@const Icon = tool.icon}
        <button class="lt-tool" aria-pressed={liteTools.retouch === tool.id} data-testid="retouch-{tool.id}" onclick={() => pick(tool.id)}>
          <Icon size={20} />
          <span>{t(tool.label)}</span>
        </button>
      {/each}
    </div>
    <p class="lt-hint" data-testid="retouch-hint">
      {t(current.hint)}
    </p>
    {#if liteTools.retouch !== "red-eye"}
      <Slider label={t("Brush size")} value={liteTools.brush} min={1} max={100} step={1} defaultValue={6} format={(v) => `${Math.round(v)}`} oninput={(v) => (liteTools.brush = v)} />
    {/if}
    {#if liteTools.retouch === "remove"}
      <div class="quality">
        <span class="label">{t("Quality")}</span>
        <div class="lt-seg" role="group" aria-label={t("Quality")}>
          <button aria-pressed={toolSettings.removeQuality === "fast"} onclick={() => (toolSettings.removeQuality = "fast")}>{t("Fast")}</button>
          <button aria-pressed={toolSettings.removeQuality === "best"} onclick={() => (toolSettings.removeQuality = "best")}>{t("Best")}</button>
        </div>
      </div>
    {/if}
  </section>

  <section class="lt-section">
    <h3 class="lt-eyebrow">{t("Subject")}</h3>
    {#if !hasSelection}
      <button class="lt-card subject" disabled={!editor.hasDocument || !!lite.working} data-testid="select-subject" onclick={() => selectSubject()}>
        <span class="lt-card__icon">
          {#if lite.working === "subject"}<LoaderCircle size={18} class="lt-spin" />{:else}<ScanFace size={18} />{/if}
        </span>
        <span>
          <span class="lt-card__title">
            {t("Select subject")}
            {#if lite.unavailable.subject}<span class="oa-badge oa-badge--sm">{t("Coming soon")}</span>{/if}
          </span>
          <span class="lt-card__text">{lite.working === "subject" && busyLabel ? busyLabel : t("Finds the person or main object, then offers what to do with it.")}</span>
        </span>
        <span></span>
      </button>
    {:else}
      <div class="context" data-testid="subject-actions">
        <div class="context__head">
          <span class="lt-card__title">{t("Subject selected")}</span>
          <button class="oa-btn oa-btn--ghost" onclick={() => lite.exec({ op: "select.none" })}><X size={14} />{t("Clear")}</button>
        </div>
        <div class="lt-chips">
          <button class="lt-chip" class:lt-chip--soon={lite.unavailable["remove-bg"]} disabled={!!lite.working} onclick={() => removeBackground()}><ImageOff size={16} />{t("Remove background")}</button>
          <button class="lt-chip" class:lt-chip--soon={lite.unavailable["blur-bg"]} disabled={!!lite.working} onclick={() => blurBackground()}><Focus size={16} />{t("Blur background")}</button>
          <button class="lt-chip" disabled={!!lite.working} onclick={() => brightenSubject()}><Sun size={16} />{t("Brighten subject")}</button>
        </div>
      </div>
    {/if}
    {#if lite.working && busyLabel}
      <div class="lt-progress" class:lt-progress--indeterminate={editor.busy?.fraction == null}>
        <span style:width={editor.busy?.fraction != null ? `${Math.round(editor.busy.fraction * 100)}%` : undefined}></span>
      </div>
    {/if}
  </section>
</div>

<style>
  .retouch {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .three {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .lt-hint {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 18px;
  }
  .quality {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--space-3);
  }
  .quality .label {
    font: var(--type-ui);
    font-weight: var(--weight-regular);
    color: var(--text-body);
  }
  .subject {
    width: 100%;
    text-align: left;
    cursor: pointer;
    transition: var(--transition-control);
  }
  .subject:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .subject:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .context {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--bg-subtle);
    border: var(--border-width) solid var(--border-hairline);
  }
  .context__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
</style>
