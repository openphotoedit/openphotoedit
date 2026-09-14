<script lang="ts">
  // Placeholder shell: replaced by the full Lite profile.
  import Canvas from "../canvas/Canvas.svelte";
  import BrandMark from "../ui/BrandMark.svelte";
  import { editor } from "../lib/editor.svelte";
  import { openFile, pickFiles, exportBlob, saveBlob } from "../lib/io";
  import { DEVELOP_DEFAULT, type Develop } from "../engine/types";

  let develop = $state<Develop>({ ...DEVELOP_DEFAULT });
  let layerId: number | null = null;

  async function open() {
    const [f] = await pickFiles();
    if (f) {
      layerId = null;
      develop = { ...DEVELOP_DEFAULT };
      await openFile(f);
    }
  }

  async function setDevelop(key: keyof Develop, value: number) {
    develop = { ...develop, [key]: value };
    const adjustment = { kind: "develop", ...develop };
    if (layerId == null) {
      const r = await editor.exec({ op: "layer.add-adjustment", adjustment, name: "Adjustments" });
      layerId = (r?.data?.id as number) ?? null;
    } else {
      await editor.exec({ op: "layer.set-adjustment", id: layerId, adjustment });
    }
  }

  async function save() {
    const blob = await exportBlob({ format: "jpeg", quality: 0.92 });
    await saveBlob(blob, `${editor.fileName}.jpg`);
  }
</script>

<header class="bar">
  <BrandMark />
  <button class="oa-btn oa-btn--secondary" data-testid="open" onclick={open}>Open</button>
  <button class="oa-btn oa-btn--ghost" onclick={() => editor.undo()}>Undo</button>
  <button class="oa-btn oa-btn--ghost" onclick={() => editor.redo()}>Redo</button>
  <span class="grow"></span>
  <button class="oa-btn oa-btn--ghost" onclick={() => editor.setProfile("pro")}>Open in Pro</button>
  <button class="oa-btn oa-btn--primary" disabled={!editor.hasDocument} onclick={save}>Export</button>
</header>
<div class="main">
  <div class="stage"><Canvas /></div>
  <aside class="side">
    {#each ["exposure", "contrast", "highlights", "shadows", "temperature", "vibrance"] as k (k)}
      <label>
        <span>{k}</span>
        <input type="range" min={k === "exposure" ? -3 : -100} max={k === "exposure" ? 3 : 100} step={k === "exposure" ? 0.05 : 1}
          value={develop[k as keyof Develop]} oninput={(e) => setDevelop(k as keyof Develop, +e.currentTarget.value)} disabled={!editor.hasDocument} />
      </label>
    {/each}
  </aside>
</div>

<style>
  .bar { display: flex; align-items: center; gap: var(--space-2); height: 52px; padding: 0 var(--space-4); border-bottom: 1px solid var(--border-hairline); }
  .grow { flex: 1; }
  .main { flex: 1; display: flex; min-height: 0; }
  .stage { flex: 1; min-width: 0; }
  .side { width: 260px; padding: var(--space-4); border-left: 1px solid var(--border-hairline); display: flex; flex-direction: column; gap: var(--space-3); }
  label { display: flex; flex-direction: column; gap: 4px; font: var(--type-caption); text-transform: capitalize; }
</style>
