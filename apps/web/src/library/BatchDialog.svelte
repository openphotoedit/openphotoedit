<script lang="ts">
  // Batch export: an optional action or develop preset, then one or more
  // export recipes, into a ZIP or a folder. Runs on a private engine so the
  // open document is untouched.
  //
  //   <BatchDialog inputs={library.batchInputs()} folder={handle} onclose={…} />
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Copy from "@lucide/svelte/icons/copy";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Check from "@lucide/svelte/icons/check";
  import { onMount } from "svelte";
  import { DEVELOP_DEFAULT, type Develop } from "../engine/types";
  import { t } from "../lib/i18n";
  import Dialog from "../ui/Dialog.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Menu from "../ui/Menu.svelte";
  import NumberField from "../ui/NumberField.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import Select from "../ui/Select.svelte";
  import Slider from "../ui/Slider.svelte";
  import type { MenuEntry } from "../ui/menu";
  import { actions } from "./actions.svelte";
  import { EXT, makeRecipe, RECIPE_PRESETS, runBatch, type BatchInput, type BatchProgress, type BatchReport, type Recipe } from "./batch";
  import { applyTemplate } from "./template";
  import "./library.css";

  let {
    inputs,
    folder = null,
    actionId = null,
    onclose,
  }: {
    inputs: BatchInput[];
    /** The library folder, offered as a destination when it can be written. */
    folder?: FileSystemDirectoryHandle | null;
    /** Preselect an action to run on each photo. */
    actionId?: string | null;
    onclose: () => void;
  } = $props();

  const RECIPES_KEY = "ops.library.recipes";

  function loadRecipes(): Recipe[] {
    try {
      const r = JSON.parse(localStorage.getItem(RECIPES_KEY) ?? "null");
      if (Array.isArray(r) && r.length) return r.map((x) => ({ ...makeRecipe({ name: "Recipe" }), ...x, resize: { ...makeRecipe({ name: "" }).resize, ...x.resize }, watermark: { ...makeRecipe({ name: "" }).watermark, ...x.watermark } }));
    } catch {
      /* fall through */
    }
    return [structuredClone(RECIPE_PRESETS[0]), { ...structuredClone(RECIPE_PRESETS[1]), enabled: false }];
  }

  let recipes = $state<Recipe[]>(loadRecipes());
  let current = $state(0);
  let edits = $state<"none" | "action" | "develop">(actionId ? "action" : "none");
  let chosenAction = $state<string | null>(actionId);
  let develop = $state<Develop>({ ...DEVELOP_DEFAULT });
  let destKind = $state<"zip" | "folder">("zip");
  let destHandle = $state<FileSystemDirectoryHandle | null>(null);
  let addMenu = $state<DOMRect | null>(null);
  let addBtn = $state<HTMLElement | null>(null);
  let progress = $state<BatchProgress | null>(null);
  let report = $state<BatchReport | null>(null);
  let failure = $state<string | null>(null);
  let abort: AbortController | null = null;
  let zipUrl = $state<string | null>(null);

  onMount(() => {
    if (!actions.loaded) void actions.load().then(() => (chosenAction ??= actions.activeId));
    else chosenAction ??= actions.activeId;
    return () => {
      abort?.abort();
      if (zipUrl) URL.revokeObjectURL(zipUrl);
    };
  });

  $effect(() => {
    const snap = $state.snapshot(recipes);
    try {
      localStorage.setItem(RECIPES_KEY, JSON.stringify(snap));
    } catch {
      /* private mode */
    }
  });

  const r = $derived(recipes[Math.min(current, recipes.length - 1)]);
  const enabled = $derived(recipes.filter((x) => x.enabled));
  const running = $derived(!!progress && !report);
  const canPickFolder = typeof window !== "undefined" && "showDirectoryPicker" in window;

  const example = $derived.by(() => {
    const first = inputs[0];
    if (!first || !r) return "";
    return `${applyTemplate(r.template, { name: first.name.replace(/\.[^.]+$/, ""), n: 1, date: first.exif?.dateTaken, w: r.resize.mode === "none" ? first.exif?.width : r.resize.mode === "long" ? r.resize.long : r.resize.width, h: r.resize.mode === "fit" || r.resize.mode === "height" ? r.resize.height : undefined, recipe: r.name, rating: first.rating, camera: first.exif?.model, ext: EXT[r.format] })}.${EXT[r.format]}`;
  });

  const addItems = $derived<MenuEntry[]>([
    ...RECIPE_PRESETS.map((p) => ({ label: p.name, run: () => add({ ...structuredClone($state.snapshot(p) as Recipe), id: Math.random().toString(36).slice(2) }) })),
    { type: "separator" as const },
    { label: t("Blank recipe"), run: () => add(makeRecipe({ name: t("Recipe {n}", { n: recipes.length + 1 }) })) },
  ]);

  function add(rec: Recipe) {
    recipes.push(rec);
    current = recipes.length - 1;
  }

  async function chooseFolder() {
    try {
      const w = window as unknown as { showDirectoryPicker: (o: unknown) => Promise<FileSystemDirectoryHandle> };
      destHandle = await w.showDirectoryPicker({ id: "ops-export", mode: "readwrite" });
      destKind = "folder";
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError")) failure = e instanceof Error ? e.message : String(e);
    }
  }

  async function useLibraryFolder() {
    const h = folder as (FileSystemDirectoryHandle & { requestPermission?: (o: unknown) => Promise<string> }) | null;
    if (!h) return;
    if ((await h.requestPermission?.({ mode: "readwrite" })) === "granted") {
      destHandle = h;
      destKind = "folder";
    } else failure = t("Write access to the library folder was not granted.");
  }

  async function start() {
    failure = null;
    if (!enabled.length) {
      failure = t("Turn on at least one export recipe.");
      return;
    }
    if (destKind === "folder" && !destHandle) {
      failure = t("Choose a destination folder first.");
      return;
    }
    const steps = edits === "action" ? (actions.sets.find((s) => s.id === chosenAction)?.steps ?? null) : null;
    if (edits === "action" && !steps) {
      failure = t("Choose an action to run.");
      return;
    }
    abort = new AbortController();
    progress = { done: 0, total: inputs.length, current: "", stage: t("Starting") };
    report = null;
    try {
      const res = await runBatch({
        inputs,
        recipes: $state.snapshot(recipes) as Recipe[],
        develop: edits === "develop" ? $state.snapshot(develop) : null,
        steps: steps ? ($state.snapshot(steps) as typeof steps) : null,
        destination: destKind === "zip" ? { kind: "zip", zipName: `export-${new Date().toISOString().slice(0, 10)}.zip` } : { kind: "folder", handle: destHandle! },
        signal: abort.signal,
        onProgress: (p) => (progress = p),
      });
      report = res;
      if (res.zip) {
        zipUrl = URL.createObjectURL(res.zip);
        download();
      }
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
      progress = null;
    }
  }

  function download() {
    if (!zipUrl || !report) return;
    const a = document.createElement("a");
    a.href = zipUrl;
    a.download = report.zipName ?? "export.zip";
    document.body.appendChild(a);
    a.click();
    a.remove();
  }

  function fmtBytes(n: number) {
    return n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`;
  }

  const RATIOS = [
    { value: "", label: "Original" },
    { value: "1:1", label: "1:1 square" },
    { value: "4:5", label: "4:5 portrait" },
    { value: "5:4", label: "5:4" },
    { value: "2:3", label: "2:3 portrait" },
    { value: "3:2", label: "3:2" },
    { value: "9:16", label: "9:16 story" },
    { value: "16:9", label: "16:9 wide" },
  ];

  const DEV_SLIDERS: [keyof Develop, string, number, number, number][] = [
    ["exposure", "Exposure", -5, 5, 0.05],
    ["contrast", "Contrast", -100, 100, 1],
    ["highlights", "Highlights", -100, 100, 1],
    ["shadows", "Shadows", -100, 100, 1],
    ["vibrance", "Vibrance", -100, 100, 1],
    ["clarity", "Clarity", -100, 100, 1],
  ];
</script>

<Dialog title={t("Export {n} photos", { n: inputs.length })} width={760} testid="lib-batch-dialog" onclose={() => (running ? abort?.abort() : onclose())} onsubmit={running || report ? undefined : start}>
  {#if report}
    <div class="done" data-testid="lib-batch-done">
      <div class="headline">
        {#if report.errors.length}<TriangleAlert size={16} />{:else}<Check size={16} />{/if}
        <span data-testid="lib-batch-summary">
          {report.cancelled ? t("Cancelled after {n} files", { n: report.outputs.length }) : t("{n} files exported, {size}", { n: report.outputs.length, size: fmtBytes(report.outputs.reduce((a, o) => a + o.bytes, 0)) })}
          {#if report.errors.length}· {t("{n} failed", { n: report.errors.length })}{/if}
        </span>
      </div>
      {#if report.zip}
        <p class="dim">{t("The ZIP downloaded as {name}.", { name: report.zipName ?? "export.zip" })}</p>
      {/if}
      <div class="table lib-scroll">
        <table>
          <thead><tr><th>{t("File")}</th><th>{t("Recipe")}</th><th class="num">{t("Size")}</th><th class="num">{t("Bytes")}</th></tr></thead>
          <tbody>
            {#each report.outputs as o (o.path)}
              <tr data-testid="lib-batch-output"><td class="mono">{o.path}</td><td>{o.recipe}</td><td class="num">{o.width} × {o.height}</td><td class="num">{fmtBytes(o.bytes)}</td></tr>
            {/each}
            {#each report.errors as e, i (i)}
              <tr class="err" data-testid="lib-batch-error"><td class="mono">{e.source}</td><td>{e.recipe ?? ""}</td><td colspan="2">{e.error}</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if report.notes.length}
        <ul class="notes">
          {#each report.notes.slice(0, 20) as n, i (i)}<li>{n}</li>{/each}
        </ul>
      {/if}
    </div>
  {:else if progress}
    <div class="running" data-testid="lib-batch-running">
      <div class="headline">{progress.stage}{#if progress.current} · <span class="mono">{progress.current}</span>{/if}</div>
      <div class="lib-progress"><span style:width="{(progress.done / Math.max(1, progress.total)) * 100}%"></span></div>
      <div class="dim">{t("{done} of {total} photos", { done: progress.done, total: progress.total })}</div>
    </div>
  {:else}
    <div class="layout oa-dense">
      <section class="col">
        <h3 class="ops-section-title">{t("Before export")}</h3>
        <SegmentedControl
          options={[
            { value: "none", label: t("No edits") },
            { value: "action", label: t("Action") },
            { value: "develop", label: t("Develop") },
          ]}
          value={edits}
          ariaLabel={t("Edits before export")}
          testid="lib-batch-edits"
          onchange={(v) => (edits = v)}
        />
        {#if edits === "action"}
          <Select options={actions.sets.map((s) => ({ value: s.id, label: s.name }))} value={chosenAction ?? ""} ariaLabel={t("Action")} testid="lib-batch-action" onchange={(v) => (chosenAction = v)} />
          {#if chosenAction}
            <p class="dim">{t("{n} steps run on each photo before the recipes.", { n: actions.sets.find((s) => s.id === chosenAction)?.steps.filter((s) => s.enabled && s.kind === "command").length ?? 0 })}</p>
          {/if}
        {:else if edits === "develop"}
          <div class="sliders">
            {#each DEV_SLIDERS as [key, label, min, max, step] (key)}
              <Slider label={t(label)} value={develop[key]} {min} {max} {step} defaultValue={0} oninput={(v) => (develop[key] = v)} />
            {/each}
          </div>
        {/if}

        <h3 class="ops-section-title gap">{t("Destination")}</h3>
        <SegmentedControl
          options={[
            { value: "zip", label: t("ZIP download") },
            { value: "folder", label: t("Folder"), disabled: !canPickFolder },
          ]}
          value={destKind}
          ariaLabel={t("Destination")}
          testid="lib-batch-dest"
          onchange={(v) => {
            destKind = v;
            if (v === "folder" && !destHandle) void chooseFolder();
          }}
        />
        {#if destKind === "folder"}
          <div class="row">
            <span class="mono grow">{destHandle?.name ?? t("No folder chosen")}</span>
            <button type="button" class="oa-btn oa-btn--secondary" onclick={chooseFolder}>{t("Choose…")}</button>
          </div>
          {#if folder}
            <button type="button" class="linkish" onclick={useLibraryFolder}>{t("Use the library folder ({name})", { name: folder.name })}</button>
          {/if}
          <p class="dim">{t("Existing files are never overwritten; new names get a number.")}</p>
        {:else if !canPickFolder}
          <p class="dim">{t("This browser cannot write to folders, so exports download as one ZIP.")}</p>
        {/if}
      </section>

      <section class="col recipes">
        <div class="row">
          <h3 class="ops-section-title grow">{t("Export recipes")}</h3>
          <span bind:this={addBtn}><button type="button" class="oa-btn oa-btn--ghost" onclick={() => (addMenu = addBtn!.getBoundingClientRect())} data-testid="lib-recipe-add"><Plus size={13} />{t("Add")}</button></span>
        </div>
        <div class="tabs" role="tablist" aria-label={t("Export recipes")}>
          {#each recipes as rec, i (rec.id)}
            <div class="tab" class:on={i === current} role="tab" tabindex="0" aria-selected={i === current} onclick={() => (current = i)} onkeydown={(e) => e.key === "Enter" && (current = i)} data-testid="lib-recipe-tab">
              <input type="checkbox" class="oa-checkbox" checked={rec.enabled} onclick={(e) => e.stopPropagation()} onchange={(e) => (rec.enabled = (e.currentTarget as HTMLInputElement).checked)} aria-label={t("Export {name}", { name: rec.name })} data-testid="lib-recipe-enabled" />
              <span class="tname">{rec.name}</span>
            </div>
          {/each}
        </div>
        {#if r}
          <div class="form" data-testid="lib-recipe-form">
            <label class="field"><span class="ops-label">{t("Name")}</span><input class="ops-field" bind:value={r.name} data-testid="lib-recipe-name" /></label>
            <div class="field">
              <span class="ops-label">{t("Format")}</span>
              <div class="row">
                <Select options={[{ value: "jpeg", label: "JPEG" }, { value: "webp", label: "WebP" }, { value: "png", label: "PNG" }]} value={r.format} ariaLabel={t("Format")} width={80} testid="lib-recipe-format" onchange={(v) => (r.format = v)} />
                {#if r.format !== "png"}
                  <span class="ops-label">{t("Quality")}</span>
                  <NumberField value={Math.round(r.quality * 100)} min={5} max={100} unit="%" width={52} ariaLabel={t("Quality")} onchange={(v) => (r.quality = v / 100)} />
                  <span class="ops-label">{t("Max")}</span>
                  <NumberField value={r.maxKB ?? 0} min={0} max={100000} unit=" KB" width={70} ariaLabel={t("Largest file size in KB, 0 for no limit")} testid="lib-recipe-maxkb" onchange={(v) => (r.maxKB = v > 0 ? v : null)} />
                {/if}
              </div>
            </div>
            <div class="field">
              <span class="ops-label">{t("Size")}</span>
              <div class="row">
                <Select
                  options={[
                    { value: "none", label: t("Original size") },
                    { value: "long", label: t("Long edge") },
                    { value: "width", label: t("Width") },
                    { value: "height", label: t("Height") },
                    { value: "fit", label: t("Fit in box") },
                  ]}
                  value={r.resize.mode}
                  ariaLabel={t("Resize")}
                  width={112}
                  testid="lib-recipe-resize"
                  onchange={(v) => (r.resize.mode = v)}
                />
                {#if r.resize.mode === "long"}
                  <NumberField value={r.resize.long} min={16} max={30000} unit=" px" width={72} ariaLabel={t("Long edge")} testid="lib-recipe-long" onchange={(v) => (r.resize.long = v)} />
                {:else if r.resize.mode === "width" || r.resize.mode === "fit"}
                  <NumberField value={r.resize.width} min={16} max={30000} unit=" px" width={72} ariaLabel={t("Width")} onchange={(v) => (r.resize.width = v)} />
                {/if}
                {#if r.resize.mode === "height" || r.resize.mode === "fit"}
                  {#if r.resize.mode === "fit"}<span class="ops-label">×</span>{/if}
                  <NumberField value={r.resize.height} min={16} max={30000} unit=" px" width={72} ariaLabel={t("Height")} onchange={(v) => (r.resize.height = v)} />
                {/if}
                {#if r.resize.mode !== "none"}
                  <label class="ops-check"><input type="checkbox" bind:checked={r.resize.upscale} />{t("Enlarge small photos")}</label>
                {/if}
              </div>
            </div>
            <div class="field">
              <span class="ops-label">{t("Crop")}</span>
              <Select options={RATIOS.map((x) => ({ ...x, label: t(x.label) }))} value={r.crop ?? ""} ariaLabel={t("Crop to ratio")} width={128} testid="lib-recipe-crop" onchange={(v) => (r.crop = v || null)} />
            </div>
            <div class="field">
              <span class="ops-label">{t("Watermark")}</span>
              <div class="row wrap">
                <label class="ops-check"><input type="checkbox" bind:checked={r.watermark.enabled} data-testid="lib-recipe-wm" />{t("Text")}</label>
                <input class="ops-field grow" bind:value={r.watermark.text} disabled={!r.watermark.enabled} placeholder={t("© Your name")} aria-label={t("Watermark text")} data-testid="lib-recipe-wm-text" />
                {#if r.watermark.enabled}
                  <NumberField value={r.watermark.size} min={1} max={20} step={0.5} unit="%" width={52} ariaLabel={t("Text size, percent of the short edge")} onchange={(v) => (r.watermark.size = v)} />
                  <NumberField value={Math.round(r.watermark.opacity * 100)} min={5} max={100} unit="%" width={52} ariaLabel={t("Opacity")} onchange={(v) => (r.watermark.opacity = v / 100)} />
                  <Select
                    options={[
                      { value: "bottom-right", label: t("Bottom right") },
                      { value: "bottom-left", label: t("Bottom left") },
                      { value: "top-right", label: t("Top right") },
                      { value: "top-left", label: t("Top left") },
                      { value: "center", label: t("Centre") },
                    ]}
                    value={r.watermark.position}
                    ariaLabel={t("Watermark position")}
                    width={104}
                    onchange={(v) => (r.watermark.position = v)}
                  />
                  <Select options={[{ value: "white", label: t("White") }, { value: "black", label: t("Black") }]} value={r.watermark.color} ariaLabel={t("Watermark colour")} width={70} onchange={(v) => (r.watermark.color = v)} />
                {/if}
              </div>
            </div>
            <label class="field"><span class="ops-label">{t("File name")}</span><input class="ops-field mono" bind:value={r.template} data-testid="lib-recipe-template" /></label>
            <div class="tokens">
              {#each ["{name}", "{n:3}", "{date}", "{w}x{h}", "{recipe}", "{camera}", "{rating}"] as tok (tok)}
                <button type="button" class="token" onclick={() => (r.template += (r.template && !r.template.endsWith("_") ? "_" : "") + tok)}>{tok}</button>
              {/each}
              <span class="example mono" data-testid="lib-recipe-example">→ {example}</span>
            </div>
            <label class="field"><span class="ops-label">{t("Subfolder")}</span><input class="ops-field" bind:value={r.folder} placeholder={t("None")} data-testid="lib-recipe-folder" /></label>
            <div class="row end">
              <IconButton label={t("Duplicate recipe")} size="sm" onclick={() => add({ ...structuredClone($state.snapshot(r) as Recipe), id: Math.random().toString(36).slice(2), name: t("{name} copy", { name: r.name }) })}><Copy size={13} /></IconButton>
              <IconButton label={t("Delete recipe")} size="sm" disabled={recipes.length < 2} onclick={() => { recipes.splice(current, 1); current = Math.max(0, current - 1); }}><Trash2 size={13} /></IconButton>
            </div>
          </div>
        {/if}
      </section>
    </div>
  {/if}
  {#if failure}<p class="failure" role="alert">{failure}</p>{/if}

  {#snippet footer()}
    {#if report}
      {#if report.zip}<button type="button" class="oa-btn oa-btn--secondary" onclick={download}>{t("Download again")}</button>{/if}
      <button type="button" class="oa-btn oa-btn--primary" onclick={onclose} data-testid="lib-batch-close">{t("Done")}</button>
    {:else if progress}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={() => abort?.abort()}>{t("Stop after this photo")}</button>
    {:else}
      <span class="dim grow">{t("{n} photos × {r} recipes = {f} files", { n: inputs.length, r: enabled.length, f: inputs.length * enabled.length })}</span>
      <button type="button" class="oa-btn oa-btn--ghost" onclick={onclose}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" disabled={!enabled.length || !inputs.length} onclick={start} data-testid="lib-batch-start">{t("Export")}</button>
    {/if}
  {/snippet}
</Dialog>

{#if addMenu}
  <Menu items={addItems} anchor={addMenu} label={t("Add recipe")} onclose={() => (addMenu = null)} />
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: var(--space-5);
    min-height: 380px;
    font: var(--type-caption);
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .recipes {
    border-left: var(--border-width) solid var(--border-hairline);
    padding-left: var(--space-5);
  }
  .gap {
    margin-top: var(--space-4);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .row.wrap {
    flex-wrap: wrap;
  }
  .row.end {
    justify-content: flex-end;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .sliders {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px;
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
    background: var(--bg-sunken);
    color: var(--text-muted);
    cursor: pointer;
  }
  .tab.on {
    background: var(--surface-active);
    border-color: var(--border-strong);
    color: var(--text-strong);
  }
  .tab .oa-checkbox {
    width: 13px;
    height: 13px;
    margin: 0;
  }
  .tname {
    white-space: nowrap;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }
  .field {
    display: grid;
    grid-template-columns: 76px 1fr;
    align-items: center;
    gap: var(--space-2);
  }
  .field .ops-field {
    width: 100%;
  }
  .tokens {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding-left: calc(76px + var(--space-2));
    margin-top: calc(var(--space-2) * -1);
  }
  .token {
    height: 20px;
    padding: 0 6px;
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    font: var(--type-caption);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    cursor: pointer;
  }
  .token:hover {
    color: var(--text-strong);
    border-color: var(--border-strong);
  }
  .example {
    margin-left: 4px;
    color: var(--text-faint);
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
  }
  .dim {
    margin: 0;
    color: var(--text-faint);
    font: var(--type-caption);
  }
  .linkish {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-muted);
    font: var(--type-caption);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .running,
  .done {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 160px;
    font: var(--type-caption);
  }
  .headline {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--weight-medium) var(--text-sm) / 1.3 var(--font-sans);
    color: var(--text-strong);
  }
  .table {
    max-height: 320px;
    overflow: auto;
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 5px 8px;
    text-align: left;
    border-bottom: var(--border-width) solid var(--border-hairline);
    white-space: nowrap;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--surface-raised);
    color: var(--text-faint);
    font-weight: var(--weight-medium);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  tr.err td {
    color: var(--danger-fg);
    white-space: normal;
  }
  .notes {
    margin: 0;
    padding-left: var(--space-4);
    color: var(--text-muted);
  }
  .failure {
    margin: var(--space-3) 0 0;
    color: var(--danger-fg);
    font: var(--type-caption);
  }
</style>
