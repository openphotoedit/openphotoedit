<script lang="ts">
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Layers from "@lucide/svelte/icons/layers";
  import BrandMark from "./BrandMark.svelte";
  import { editor } from "../lib/editor.svelte";
</script>

<main class="chooser">
  <BrandMark size={28} />
  <h1>How do you like to edit?</h1>
  <p class="lede">You can switch at any time, and nothing you make is lost when you do.</p>
  <div class="options">
    <button class="option" data-testid="choose-lite" onclick={() => editor.setProfile("lite")}>
      <span class="chip"><Sparkles size={18} /></span>
      <span class="title">Simple</span>
      <span class="desc">Tap a fix: brighten, crop, erase things, blur the background, add arrows and text.</span>
    </button>
    <button class="option" data-testid="choose-pro" onclick={() => editor.setProfile("pro")}>
      <span class="chip"><Layers size={18} /></span>
      <span class="title">Professional</span>
      <span class="desc">Layers, masks, blend modes, selections and PSD files, with Photoshop's shortcuts.</span>
    </button>
  </div>
  <p class="foot">Runs in your browser. Your photos never leave this device.</p>
</main>

<style>
  .chooser {
    margin: auto;
    padding: var(--space-8) var(--space-6);
    max-width: 640px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-4);
  }
  h1 {
    font: var(--type-h2);
    color: var(--text-strong);
    margin: var(--space-4) 0 0;
    text-wrap: balance;
  }
  .lede {
    color: var(--text-muted);
    margin: 0;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--space-4);
    width: 100%;
    margin-top: var(--space-4);
  }
  .option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    text-align: left;
    padding: var(--space-5);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-hairline);
    background: var(--surface-card);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .option:hover {
    border-color: var(--border-strong);
    background: var(--surface-hover);
  }
  .chip {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--brand) 16%, transparent);
    color: var(--text-strong);
  }
  .title {
    font: var(--weight-medium) var(--text-md) / 1.2 var(--font-sans);
    color: var(--text-strong);
  }
  .desc {
    color: var(--text-muted);
    font: var(--type-caption);
    font-size: var(--text-sm);
  }
  .foot {
    font: var(--type-caption);
    color: var(--text-faint);
    margin-top: var(--space-6);
  }
</style>
