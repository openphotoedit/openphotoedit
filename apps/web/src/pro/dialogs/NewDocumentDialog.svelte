<script lang="ts">
  // File › New: presets on the left, the exact size, resolution and
  // background on the right. Opens in a new tab when a document is open.
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Dialog from "../../ui/Dialog.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import Select from "../../ui/Select.svelte";
  import ColorSwatch from "../../ui/ColorSwatch.svelte";
  import { pro } from "../state.svelte";

  interface Preset {
    group: string;
    name: string;
    w: number;
    h: number;
    res: number;
  }
  const PRESETS: Preset[] = [
    { group: t("Photo"), name: t("Default Photoshop size"), w: 2100, h: 1500, res: 300 },
    { group: t("Photo"), name: t("Landscape 6 × 4 in"), w: 1800, h: 1200, res: 300 },
    { group: t("Photo"), name: t("Portrait 4 × 6 in"), w: 1200, h: 1800, res: 300 },
    { group: t("Print"), name: t("A4"), w: 2480, h: 3508, res: 300 },
    { group: t("Print"), name: t("US Letter"), w: 2550, h: 3300, res: 300 },
    { group: t("Screen"), name: t("HD 1080p"), w: 1920, h: 1080, res: 72 },
    { group: t("Screen"), name: t("4K UHD"), w: 3840, h: 2160, res: 72 },
    { group: t("Screen"), name: t("Web large"), w: 1920, h: 1200, res: 72 },
    { group: t("Social"), name: t("Square post"), w: 1080, h: 1080, res: 72 },
    { group: t("Social"), name: t("Portrait post"), w: 1080, h: 1350, res: 72 },
    { group: t("Social"), name: t("Story"), w: 1080, h: 1920, res: 72 },
    { group: t("Social"), name: t("Link preview"), w: 1200, h: 630, res: 72 },
  ];

  let selected = $state(0);
  let width = $state(PRESETS[0].w);
  let height = $state(PRESETS[0].h);
  let resolution = $state(PRESETS[0].res);
  let background = $state<"white" | "black" | "background" | "transparent">("white");
  let name = $state(t("Untitled"));

  function pick(i: number) {
    selected = i;
    width = PRESETS[i].w;
    height = PRESETS[i].h;
    resolution = PRESETS[i].res;
  }

  const bgColor = $derived(
    background === "white" ? { r: 255, g: 255, b: 255, a: 255 } : background === "black" ? { r: 0, g: 0, b: 0, a: 255 } : background === "background" ? editor.secondary : null,
  );

  async function create() {
    pro.close();
    if (editor.hasDocument) await editor.newTab();
    const r = await editor.exec({ op: "doc.new", width, height, background: bgColor, resolution });
    if (r) {
      editor.fileName = name.trim() || t("Untitled");
      editor.hasDocument = true;
      editor.dirty = false;
      pro.selectedIds = [];
      requestAnimationFrame(() => editor.fit());
    }
  }
</script>

<Dialog title={t("New Document")} width={600} testid="new-dialog" onclose={() => pro.close()} onsubmit={create}>
  <div class="layout">
    <div class="presets" role="listbox" aria-label={t("Presets")}>
      {#each PRESETS as p, i (p.name)}
        {#if i === 0 || PRESETS[i - 1].group !== p.group}
          <span class="ops-section-title head">{p.group}</span>
        {/if}
        <button type="button" role="option" aria-selected={selected === i} class:on={selected === i} onclick={() => pick(i)}>
          <span class="shape" style:aspect-ratio="{p.w} / {p.h}"></span>
          <span class="text">
            <span class="name">{p.name}</span>
            <span class="dim">{p.w} × {p.h} px · {p.res} ppi</span>
          </span>
        </button>
      {/each}
    </div>
    <div class="details ops-stack">
      <label class="field">
        <span class="ops-label">{t("Name")}</span>
        <input class="ops-field" bind:value={name} aria-label={t("Name")} />
      </label>
      <div class="ops-row">
        <span class="ops-label lbl">{t("Width")}</span>
        <NumberField value={width} min={1} max={300000} unit="px" width={96} ariaLabel={t("Width")} testid="new-width" onchange={(v) => ((width = v), (selected = -1))} />
      </div>
      <div class="ops-row">
        <span class="ops-label lbl">{t("Height")}</span>
        <NumberField value={height} min={1} max={300000} unit="px" width={96} ariaLabel={t("Height")} testid="new-height" onchange={(v) => ((height = v), (selected = -1))} />
      </div>
      <div class="ops-row">
        <span class="ops-label lbl">{t("Resolution")}</span>
        <NumberField value={resolution} min={1} max={10000} unit="ppi" width={96} ariaLabel={t("Resolution")} onchange={(v) => (resolution = v)} />
      </div>
      <div class="ops-row">
        <span class="ops-label lbl">{t("Background")}</span>
        <Select
          ariaLabel={t("Background contents")}
          value={background}
          width={140}
          options={[
            { value: "white", label: t("White") },
            { value: "black", label: t("Black") },
            { value: "background", label: t("Background colour") },
            { value: "transparent", label: t("Transparent") },
          ]}
          onchange={(v) => (background = v)}
        />
        <ColorSwatch color={bgColor} size={20} />
      </div>
      <p class="ops-note">{t("{w} × {h} px, {mb} in memory.", { w: width, h: height, mb: `${((width * height * 4) / 1048576).toFixed(1)} MB` })}</p>
    </div>
  </div>
  {#snippet footer()}
    <button type="button" class="oa-btn oa-btn--secondary" onclick={() => pro.close()}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={create}>{t("Create")}</button>
  {/snippet}
</Dialog>

<style>
  .layout {
    display: grid;
    grid-template-columns: 1fr 230px;
    gap: 16px;
    min-height: 320px;
  }
  .presets {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 380px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .head {
    padding: 8px 4px 4px;
  }
  .head:first-child {
    padding-top: 0;
  }
  .presets button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: transparent;
    text-align: left;
    cursor: default;
  }
  .presets button:hover {
    background: var(--surface-hover);
  }
  .presets button.on {
    background: var(--surface-active);
    border-color: var(--border-strong);
  }
  .shape {
    height: 22px;
    max-width: 34px;
    border: 1px solid var(--text-muted);
    border-radius: 1px;
    flex: 0 0 auto;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font: var(--weight-medium) var(--text-xs) / 1.2 var(--font-sans);
    color: var(--text-strong);
  }
  .dim {
    font: var(--type-caption);
    font-size: var(--text-2xs);
    color: var(--text-faint);
  }
  .details {
    padding-left: 16px;
    border-left: 1px solid var(--border-hairline);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .lbl {
    width: 72px;
  }
</style>
