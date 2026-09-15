<script lang="ts">
  // No document yet: one big place to put a photo, or a blank canvas.
  import ImageUp from "@lucide/svelte/icons/image-up";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import RectangleVertical from "@lucide/svelte/icons/rectangle-vertical";
  import Smartphone from "@lucide/svelte/icons/smartphone";
  import Monitor from "@lucide/svelte/icons/monitor";
  import Plus from "@lucide/svelte/icons/plus";
  import Layers from "@lucide/svelte/icons/layers";
  import { editor } from "../lib/editor.svelte";
  import { newDocument } from "../lib/io";
  import { t } from "../lib/i18n";
  import BrandMark from "../ui/BrandMark.svelte";
  import { openPhoto } from "./actions";

  let over = $state(false);
  let custom = $state(false);
  let cw = $state(1600);
  let ch = $state(1200);
  const isMac = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);

  const SIZES = [
    { id: "post", label: "Instagram post", w: 1080, h: 1350, icon: RectangleVertical },
    { id: "story", label: "Story", w: 1080, h: 1920, icon: Smartphone },
    { id: "wide", label: "16:9", w: 1920, h: 1080, icon: Monitor },
  ];

  function blank(w: number, h: number) {
    return newDocument(Math.max(1, Math.min(12000, Math.round(w))), Math.max(1, Math.min(12000, Math.round(h))), { r: 255, g: 255, b: 255, a: 255 });
  }
</script>

<main class="start" data-testid="lite-start">
  <header class="top">
    <BrandMark size={20} />
    <button class="oa-btn oa-btn--ghost" onclick={() => editor.setProfile("pro")}>
      <Layers size={14} />
      {t("Switch to Pro")}
    </button>
  </header>

  <div class="center">
    <button
      class="drop"
      class:over
      data-testid="open"
      onclick={openPhoto}
      ondragenter={() => (over = true)}
      ondragleave={() => (over = false)}
      ondrop={() => (over = false)}
    >
      <span class="icon"><ImageUp size={28} strokeWidth={1.75} /></span>
      <span class="title">{t("Open a photo")}</span>
      <span class="sub">{t("Drop it here, paste it, or choose a file")}</span>
      <span class="oa-btn oa-btn--primary oa-btn--md cta">{t("Choose a photo")}</span>
      <span class="kbd">{isMac ? "⌘O" : "Ctrl+O"}</span>
    </button>

    <section class="blank" aria-labelledby="blank-title">
      <h2 id="blank-title" class="lt-eyebrow">{t("Or start blank")}</h2>
      <div class="sizes">
        {#each SIZES as s (s.id)}
          {@const Icon = s.icon}
          <button class="size" data-testid="blank-{s.id}" onclick={() => blank(s.w, s.h)}>
            <Icon size={18} />
            <span class="name">{t(s.label)}</span>
            <span class="dim">{s.w} × {s.h}</span>
          </button>
        {/each}
        <button class="size" aria-expanded={custom} onclick={() => (custom = !custom)}>
          <Plus size={18} />
          <span class="name">{t("Custom")}</span>
          <span class="dim">{t("Any size")}</span>
        </button>
      </div>
      {#if custom}
        <form class="custom" onsubmit={(e) => { e.preventDefault(); blank(cw, ch); }}>
          <label><span>{t("Width")}</span><input class="oa-input" type="number" min="1" max="12000" bind:value={cw} /></label>
          <span class="x">×</span>
          <label><span>{t("Height")}</span><input class="oa-input" type="number" min="1" max="12000" bind:value={ch} /></label>
          <button class="oa-btn oa-btn--secondary oa-btn--md" type="submit">{t("Create")}</button>
        </form>
      {/if}
    </section>
  </div>

  <p class="privacy"><ShieldCheck size={14} />{t("Everything happens in your browser. Your photos never leave this device.")}</p>
</main>

<style>
  .start {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--bg-page);
    padding: 0 var(--space-6) calc(var(--space-6) + var(--safe-bottom));
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--topbar-h);
    flex: 0 0 auto;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: var(--space-8);
    width: 100%;
    max-width: 640px;
    margin: 0 auto;
    padding-block: var(--space-6);
  }
  .drop {
    position: relative;
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-12) var(--space-6);
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius-2xl);
    background: var(--surface-card);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .drop:hover,
  .drop.over {
    border-color: var(--text-muted);
    background: var(--bg-subtle);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    border-radius: var(--radius-xl);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
    color: var(--text-strong);
    margin-bottom: var(--space-2);
  }
  .title {
    font: var(--type-h3);
    letter-spacing: var(--tracking-heading);
    color: var(--text-strong);
  }
  .sub {
    font: var(--type-body);
    color: var(--text-muted);
  }
  .cta {
    margin-top: var(--space-4);
    pointer-events: none;
  }
  .kbd {
    position: absolute;
    top: var(--space-4);
    right: var(--space-4);
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
  }
  .blank {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .sizes {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .size {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    border: var(--border-width) solid var(--border-hairline);
    background: var(--surface-card);
    color: var(--text-muted);
    cursor: pointer;
    text-align: left;
    transition: var(--transition-control);
  }
  .size:hover,
  .size[aria-expanded="true"] {
    border-color: var(--border-strong);
    color: var(--text-strong);
  }
  .name {
    font: var(--type-ui);
    color: var(--text-strong);
    margin-top: var(--space-1);
  }
  .dim {
    font: var(--type-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
  }
  .custom {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }
  .custom label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .custom input {
    height: var(--control-h-md);
  }
  .x {
    padding-bottom: 8px;
    color: var(--text-faint);
  }
  .privacy {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    margin: 0;
    font: var(--type-caption);
    color: var(--text-faint);
    text-align: center;
  }
  @media (max-width: 599px) {
    .start {
      padding-inline: var(--space-4);
    }
    .drop {
      padding: var(--space-10) var(--space-4);
    }
    .kbd {
      display: none;
    }
    .sizes {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .center {
      gap: var(--space-6);
      justify-content: flex-start;
      padding-top: var(--space-4);
    }
  }
</style>
