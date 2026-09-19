<script lang="ts">
  // A filter's parameters with a live preview on the canvas. The preview is
  // applied and taken back through history; Cancel leaves the document as it
  // was. On a smart object the filter is added (or edited) as a smart filter.
  import { onDestroy } from "svelte";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import Dialog from "../../ui/Dialog.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";
  import Slider from "../../ui/Slider.svelte";
  import { PreviewSession } from "../engine.svelte";
  import { buildCommand, defaultParams, type FilterDef } from "../filters";
  import { pro } from "../state.svelte";

  let { def, smart }: { def: FilterDef; smart?: { id: number; index?: number; initial?: Record<string, unknown> } } = $props();

  function initialValues(): Record<string, unknown> {
    const base = defaultParams(def, editor.summary!);
    const init = smart?.initial;
    if (init) {
      for (const p of def.params) if (init[p.key] !== undefined) base[p.key] = p.type === "choice" ? String(init[p.key]) : init[p.key];
      if (def.op === "filter.custom" && Array.isArray(init.kernel)) (init.kernel as number[]).forEach((k, i) => (base[`k${i}`] = k));
      return base;
    }
    if (pro.lastFilter?.def.op === def.op) return { ...base, ...pro.lastFilter.params };
    return base;
  }

  let values = $state<Record<string, unknown>>(initialValues());
  let preview = $state(true);
  let closing = false;
  const session = new PreviewSession();

  // Add Noise gets its own seed each time the dialog opens (kept while it is
  // open, so the preview is stable), so repeated applications do not stack
  // the same pattern. A smart filter being edited keeps its seed.
  const pickSeed = () => (typeof smart?.initial?.seed === "number" ? (smart.initial.seed as number) : crypto.getRandomValues(new Uint32Array(1))[0]);
  const noiseSeed = pickSeed();
  const extra = () => (def.op === "filter.clouds" ? { fg: editor.primary, bg: editor.secondary } : def.op === "filter.add-noise" ? { seed: noiseSeed } : {});
  const command = () => buildCommand(def, $state.snapshot(values), extra());

  async function apply() {
    const cmd = command();
    if (smart && smart.index == null) await session.exec({ op: "layer.smart-filter-add", id: smart.id, filter: cmd });
    else if (smart) await session.exec({ op: "layer.smart-filter-set", id: smart.id, index: smart.index, filter: cmd });
    else await session.exec(cmd);
  }

  let timer = 0;
  $effect(() => {
    const key = JSON.stringify(values);
    const on = preview;
    clearTimeout(timer);
    if (closing) return;
    if (!on) {
      void session.cancel();
      return;
    }
    timer = window.setTimeout(() => session.request(key, apply), 110);
  });

  onDestroy(() => clearTimeout(timer));

  async function ok() {
    closing = true;
    clearTimeout(timer);
    const done = await session.commit(JSON.stringify(values), apply);
    if (done) {
      if (!smart || smart.index == null) pro.lastFilter = { def, params: $state.snapshot(values) };
      pro.close();
    } else closing = false;
  }

  async function cancel() {
    closing = true;
    clearTimeout(timer);
    await session.cancel();
    pro.close();
  }

  const isKernel = $derived(def.op === "filter.custom");
</script>

<Dialog title={smart ? t("{name} (smart filter)", { name: def.label }) : def.label} width={isKernel ? 400 : 340} scrim="clear" align="right" testid="filter-dialog" onclose={cancel} onsubmit={ok}>
  <div class="ops-stack">
    {#if def.note}<p class="ops-note">{def.note}</p>{/if}
    {#if isKernel}
      <div class="kernel">
        {#each Array.from({ length: 9 }, (_, i) => i) as i (i)}
          <NumberField value={Number(values[`k${i}`])} min={-999} max={999} width={56} ariaLabel={t("Kernel {i}", { i: i + 1 })} onchange={(v) => (values[`k${i}`] = v)} />
        {/each}
      </div>
      <div class="ops-row">
        <NumberField label={t("Scale")} value={Number(values.scale)} min={1} max={9999} width={56} onchange={(v) => (values.scale = v)} />
        <NumberField label={t("Offset")} value={Number(values.offset)} min={-9999} max={9999} width={56} onchange={(v) => (values.offset = v)} />
      </div>
    {:else}
      {#each def.params as p (p.key)}
        {#if p.type === "number"}
          {@const wide = p.max - p.min > 5000}
          {#if wide}
            <div class="ops-row">
              <span class="ops-label grow">{p.label}</span>
              <NumberField value={Number(values[p.key])} min={p.min} max={p.max} step={p.step ?? 1} unit={p.unit} width={80} ariaLabel={p.label} onchange={(v) => (values[p.key] = v)} />
            </div>
          {:else}
            <Slider
              label={p.label}
              value={Number(values[p.key])}
              min={p.min}
              max={p.max}
              step={p.step ?? 1}
              unit={p.unit}
              fieldWidth={62}
              testid="filter-param-{p.key}"
              oninput={(v) => (values[p.key] = v)}
            />
          {/if}
        {:else if p.type === "bool"}
          <label class="ops-check"><input type="checkbox" checked={Boolean(values[p.key])} onchange={(e) => (values[p.key] = e.currentTarget.checked)} />{p.label}</label>
        {:else if p.options.length <= 3}
          <div class="ops-row">
            <span class="ops-label grow">{p.label}</span>
            <SegmentedControl ariaLabel={p.label} options={p.options} value={String(values[p.key])} onchange={(v) => (values[p.key] = v)} />
          </div>
        {:else}
          <div class="ops-row">
            <span class="ops-label grow">{p.label}</span>
            <Select ariaLabel={p.label} options={p.options} value={String(values[p.key])} onchange={(v) => (values[p.key] = v)} />
          </div>
        {/if}
      {/each}
    {/if}
    {#if session.error}<p class="ops-error" role="alert">{session.error}</p>{/if}
  </div>
  {#snippet footer()}
    <label class="ops-check preview">
      <input type="checkbox" bind:checked={preview} data-testid="filter-preview" />{t("Preview")}
      {#if session.busy}<span class="spinner" aria-label={t("Rendering preview")}></span>{/if}
    </label>
    <button type="button" class="oa-btn oa-btn--secondary" onclick={cancel}>{t("Cancel")}</button>
    <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={ok}>{t("OK")}</button>
  {/snippet}
</Dialog>

<style>
  .kernel {
    display: grid;
    grid-template-columns: repeat(3, auto);
    justify-content: center;
    gap: 4px;
  }
  .grow {
    flex: 1;
  }
  .preview {
    margin-right: auto;
  }
  .spinner {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong);
    border-top-color: var(--text-strong);
    animation: oa-spin 0.8s linear infinite;
  }
</style>
