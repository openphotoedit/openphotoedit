<script lang="ts">
  // The library screen: browse a folder in place, rate and flag from the
  // keyboard, compare and survey, run assisted culling, and hand files to the
  // editor or to a batch export. Mount it anywhere with a height.
  //
  //   <LibraryView active onopen={(file, item) => openFile(file)} />
  import { onMount } from "svelte";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Square from "@lucide/svelte/icons/square";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
  import ScanEye from "@lucide/svelte/icons/scan-eye";
  import FlagOff from "@lucide/svelte/icons/flag-off";
  import FolderOutput from "@lucide/svelte/icons/folder-output";
  import Settings2 from "@lucide/svelte/icons/settings-2";
  import ArrowUpDown from "@lucide/svelte/icons/arrow-up-down";
  import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
  import Star from "@lucide/svelte/icons/star";
  import X from "@lucide/svelte/icons/x";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import Keyboard from "@lucide/svelte/icons/keyboard";
  import { t } from "../lib/i18n";
  import IconButton from "../ui/IconButton.svelte";
  import Menu from "../ui/Menu.svelte";
  import Select from "../ui/Select.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import type { MenuEntry } from "../ui/menu";
  import { tooltip } from "../ui/tooltip";
  import BatchDialog from "./BatchDialog.svelte";
  import Compare from "./components/Compare.svelte";
  import Filmstrip from "./components/Filmstrip.svelte";
  import Grid from "./components/Grid.svelte";
  import MetaPanel from "./components/MetaPanel.svelte";
  import Survey from "./components/Survey.svelte";
  import Viewer, { type ViewerState } from "./components/Viewer.svelte";
  import { DEFAULT_FILTERS, library, type FlagFilter, type SortKey, type ViewMode } from "./store.svelte";
  import type { LibraryItem } from "./types";
  import { LABEL_NAMES } from "./xmp";
  import "./library.css";

  let {
    active = true,
    onopen,
    showSidebar = true,
  }: {
    /** Keyboard shortcuts are live only while the library is the visible screen. */
    active?: boolean;
    /** Open a photo in the editor (D, double-click). */
    onopen?: (file: File, item: LibraryItem) => void;
    showSidebar?: boolean;
  } = $props();

  let columns = $state(1);
  let input: HTMLInputElement;
  let settingsMenu = $state<DOMRect | null>(null);
  let settingsBtn = $state<HTMLElement | null>(null);
  let batchOpen = $state(false);
  let helpOpen = $state(false);
  let loupeView = $state<ViewerState>({ zoom: "fit", cx: 0.5, cy: 0.5 });
  let pair = $state<[string | null, string | null]>([null, null]);
  let survey = $state<string[]>([]);
  let recent = $state<string | null>(null);

  onMount(() => {
    void library.recentFolderName().then((n) => (recent = n));
  });

  async function openFolder() {
    try {
      if (library.supportsPicker) await library.openWithPicker();
      else input.click();
    } catch (e) {
      library.notify(e instanceof Error ? e.message : String(e), "error");
    }
  }

  async function openItem(it: LibraryItem | null) {
    if (!it || !onopen) return;
    onopen(await library.fileOf(it), it);
  }

  function setMode(m: ViewMode) {
    const v = library.view;
    if (!v.length) return;
    if (!library.focused) library.select(v[0].path);
    const f = library.focused!;
    if (m === "compare") {
      const sel = library.selectedItems();
      const other = sel.find((i) => i.path !== f.path) ?? v[(library.focusIndex + 1) % v.length];
      pair = [f.path, other && other.path !== f.path ? other.path : null];
    } else if (m === "survey") {
      const sel = library.selectedItems();
      survey = (sel.length > 1 ? sel : v.slice(library.focusIndex, library.focusIndex + 6)).slice(0, 24).map((i) => i.path);
    } else if (m === "loupe") loupeView = { zoom: "fit", cx: 0.5, cy: 0.5 };
    library.mode = m;
  }

  function stepCandidate(delta: number) {
    const v = library.view;
    const cand = pair[1] ?? pair[0];
    let i = v.findIndex((x) => x.path === cand);
    for (let k = 0; k < v.length; k++) {
      i = (i + delta + v.length) % v.length;
      if (v[i].path !== pair[0]) break;
    }
    pair = [pair[0], v[i]?.path ?? null];
  }

  function isTyping(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (!el) return false;
    if (el.closest(".ops-dialog, .ops-menu, [role=dialog], [role=menu]")) return true;
    return el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName);
  }

  function onKey(e: KeyboardEvent) {
    if (!active || batchOpen || isTyping(e) || !library.items.length) return;
    const mod = e.metaKey || e.ctrlKey;
    const k = e.key;
    const lower = k.length === 1 ? k.toLowerCase() : k;
    // Shift+key applies a mark and advances, whatever the auto-advance setting.
    const advance = e.shiftKey ? true : library.settings.autoAdvance;
    let handled = true;
    if (mod && lower === "a") library.selectAll();
    else if (mod && lower === "z") library.undo();
    else if (mod) handled = false;
    else if (/^Digit[0-5]$/.test(e.code)) library.setRating(Number(e.code.slice(5)), advance);
    else if (/^Digit[6-9]$/.test(e.code)) library.toggleLabel(Number(e.code.slice(5)) - 5, advance);
    else if (lower === "p") library.setFlag(1, advance);
    else if (lower === "x") library.setFlag(-1, advance);
    else if (lower === "u") library.setFlag(0, advance);
    else if (lower === "a" && !e.shiftKey) {
      library.settings.autoAdvance = !library.settings.autoAdvance;
      library.notify(library.settings.autoAdvance ? t("Auto-advance is on: marking a photo moves to the next.") : t("Auto-advance is off."));
    } else if (k === "ArrowRight" || k === "ArrowLeft") {
      if (library.mode === "compare") stepCandidate(k === "ArrowRight" ? 1 : -1);
      else library.move(k === "ArrowRight" ? 1 : -1, e.shiftKey);
    } else if (k === "ArrowDown" || k === "ArrowUp") {
      library.move((k === "ArrowDown" ? 1 : -1) * (library.mode === "grid" ? columns : 1), e.shiftKey);
    } else if (k === "Home") library.move(-library.view.length);
    else if (k === "End") library.move(library.view.length);
    else if (k === " ") {
      if (library.mode === "grid") setMode("loupe");
      else if (library.mode === "loupe") loupeView = loupeView.zoom === "fit" ? { zoom: 1, cx: 0.5, cy: 0.5 } : { zoom: "fit", cx: 0.5, cy: 0.5 };
      else setMode("grid");
    } else if (lower === "e" || (k === "Enter" && library.mode === "grid")) setMode("loupe");
    else if (lower === "z" && library.mode === "loupe") loupeView = loupeView.zoom === "fit" ? { zoom: 1, cx: 0.5, cy: 0.5 } : { zoom: "fit", cx: 0.5, cy: 0.5 };
    else if (lower === "g" || (k === "Escape" && library.mode !== "grid")) setMode("grid");
    else if (lower === "c") setMode("compare");
    else if (lower === "n") setMode("survey");
    else if (lower === "d") void openItem(library.focused);
    else if (k === "?") helpOpen = !helpOpen;
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  const modeOptions = $derived<{ value: ViewMode; label: string }[]>([
    { value: "grid", label: t("Grid") },
    { value: "loupe", label: t("Loupe") },
    { value: "compare", label: t("Compare") },
    { value: "survey", label: t("Survey") },
  ]);

  const settingsItems = $derived<MenuEntry[]>([
    { label: t("Auto-advance after marking"), shortcut: "A", checked: library.settings.autoAdvance, testid: "lib-set-advance", run: () => (library.settings.autoAdvance = !library.settings.autoAdvance) },
    {
      label: t("Write ratings to XMP sidecars"),
      checked: library.settings.writeXmp,
      disabled: library.folder?.source === "input",
      hint: t("Open the folder with Open folder (Chrome, Edge) to allow writing"),
      run: () => void library.enableXmp(!library.settings.writeXmp),
    },
    { label: t("Suggest rejects includes similar shots"), checked: library.settings.rejectDuplicates, run: () => (library.settings.rejectDuplicates = !library.settings.rejectDuplicates) },
    { type: "separator" },
    { label: t("Keyboard shortcuts"), shortcut: "?", run: () => (helpOpen = true) },
  ]);

  const filtersActive = $derived(JSON.stringify(library.filters) !== JSON.stringify(DEFAULT_FILTERS));
  const shortcuts: [string, string][] = [
    ["0–5", "Rate"],
    ["P / X / U", "Pick, reject, unflag"],
    ["6–9", "Red, yellow, green, blue label"],
    ["Shift + key", "Mark and move to the next photo"],
    ["A", "Auto-advance on or off"],
    ["← → ↑ ↓", "Move (Shift extends the selection)"],
    ["G / E / C / N", "Grid, loupe, compare, survey"],
    ["Space / Z", "Loupe; zoom to 100%"],
    ["D", "Open in the editor"],
    ["⌘/Ctrl + Z", "Undo a mark"],
  ];
</script>

<svelte:window onkeydown={onKey} />

<div class="lib-root lib" data-testid="library">
  <input
    bind:this={input}
    type="file"
    class="visually-hidden"
    webkitdirectory
    multiple
    data-testid="lib-folder-input"
    tabindex="-1"
    aria-hidden="true"
    onchange={() => {
      if (input.files?.length) void library.openFileList(input.files);
      input.value = "";
    }}
  />

  <header class="bar">
    <button type="button" class="oa-btn oa-btn--secondary" onclick={openFolder} data-testid="lib-open">
      <FolderOpen size={14} />{t("Open folder")}
    </button>
    {#if library.folder}
      <div class="folder" use:tooltip={library.folder.source === "input" ? t("Opened without write access: marks are kept in this browser only") : t("Browsing in place; nothing is imported or copied")}>
        <span class="fname">{library.folder.name}</span>
        <span class="count" data-testid="lib-count">{t("{shown} of {total}", { shown: library.view.length, total: library.items.length })}</span>
      </div>
    {/if}
    <span class="sep"></span>
    <SegmentedControl options={modeOptions} value={library.mode} ariaLabel={t("View")} testid="lib-mode" onchange={setMode} />
    <span class="grow"></span>
    {#if library.cullRun}
      <div class="cullrun" data-testid="lib-cull-progress">
        <span class="stage">{library.cullRun.stage} · {library.cullRun.done}/{library.cullRun.total}</span>
        <div class="lib-progress" style:width="120px"><span style:width="{(library.cullRun.done / Math.max(1, library.cullRun.total)) * 100}%"></span></div>
        <IconButton label={t("Cancel culling")} size="sm" onclick={() => library.cancelCull()}><X size={13} /></IconButton>
      </div>
    {:else}
      <button type="button" class="oa-btn oa-btn--ghost" disabled={!library.view.length} onclick={() => library.runCull()} data-testid="lib-cull" use:tooltip={t("Check focus, exposure, faces and similar shots on this device")}>
        <ScanEye size={14} />{t("Assisted culling")}
      </button>
    {/if}
    <button type="button" class="oa-btn oa-btn--ghost" disabled={!library.cull.size || !!library.cullRun} onclick={() => library.suggestRejects()} data-testid="lib-suggest" use:tooltip={t("Flag soft, badly exposed and eyes-closed shots as rejected. Nothing is deleted.")}>
      <FlagOff size={14} />{t("Suggest rejects")}
    </button>
    <button type="button" class="oa-btn oa-btn--ghost" disabled={!library.view.length || !onopen} onclick={() => openItem(library.focused)} use:tooltip={{ text: t("Open in editor"), shortcut: "D" }}>
      <SquarePen size={14} />{t("Edit")}
    </button>
    <button type="button" class="oa-btn oa-btn--primary" disabled={!library.view.length} onclick={() => (batchOpen = true)} data-testid="lib-batch">
      <FolderOutput size={14} />{t("Export {n}", { n: library.selectedItems().length })}
    </button>
    <span bind:this={settingsBtn}>
      <IconButton label={t("Library settings")} onclick={() => (settingsMenu = settingsBtn!.getBoundingClientRect())} testid="lib-settings"><Settings2 size={15} /></IconButton>
    </span>
  </header>

  {#if library.items.length}
    <div class="filters" data-testid="lib-filters">
      <span class="ops-label">{t("Sort")}</span>
      <Select
        options={[
          { value: "name", label: t("File name") },
          { value: "date", label: t("Capture time") },
          { value: "rating", label: t("Rating") },
          { value: "size", label: t("File size") },
        ]}
        value={library.settings.sort}
        ariaLabel={t("Sort by")}
        width={116}
        testid="lib-sort"
        onchange={(v: SortKey) => (library.settings.sort = v)}
      />
      <IconButton label={library.settings.sortDesc ? t("Descending") : t("Ascending")} size="sm" onclick={() => (library.settings.sortDesc = !library.settings.sortDesc)}>
        {#if library.settings.sortDesc}<ArrowDownUp size={13} />{:else}<ArrowUpDown size={13} />{/if}
      </IconButton>
      <span class="sep"></span>
      <span class="ops-label">{t("Rating")}</span>
      <div class="stars" role="radiogroup" aria-label={t("Minimum rating")} data-testid="lib-filter-rating">
        {#each [1, 2, 3, 4, 5] as r (r)}
          <button type="button" role="radio" aria-checked={library.filters.minRating === r} class:on={library.filters.minRating >= r} aria-label={t("At least {n} stars", { n: r })} onclick={() => (library.filters.minRating = library.filters.minRating === r ? 0 : r)}>
            <Star size={12} fill={library.filters.minRating >= r ? "currentColor" : "none"} />
          </button>
        {/each}
      </div>
      <span class="sep"></span>
      <SegmentedControl
        options={[
          { value: "all", label: t("All") },
          { value: "picked", label: t("Picked") },
          { value: "unflagged", label: t("Unflagged") },
          { value: "not-rejected", label: t("Not rejected") },
          { value: "rejected", label: t("Rejected") },
        ]}
        value={library.filters.flag}
        ariaLabel={t("Flag filter")}
        size="xs"
        testid="lib-filter-flag"
        onchange={(v: FlagFilter) => (library.filters.flag = v)}
      />
      <span class="sep"></span>
      <div class="labels" role="group" aria-label={t("Colour label filter")}>
        {#each [1, 2, 3, 4, 5] as l (l)}
          <button
            type="button"
            class:on={library.filters.labels.includes(l)}
            aria-pressed={library.filters.labels.includes(l)}
            aria-label={t(LABEL_NAMES[l])}
            use:tooltip={t("Only {label}", { label: t(LABEL_NAMES[l]).toLowerCase() })}
            onclick={() => (library.filters.labels = library.filters.labels.includes(l) ? library.filters.labels.filter((x) => x !== l) : [...library.filters.labels, l])}
          >
            <span class="lib-dot" data-label={l}></span>
          </button>
        {/each}
      </div>
      <Select
        options={[
          { value: "", label: t("Any type") },
          { value: "image", label: "JPEG, PNG, WebP…" },
          { value: "raw", label: t("Raw") },
          { value: "psd", label: "PSD" },
          { value: "tiff", label: "TIFF" },
          { value: "heif", label: "HEIC" },
        ]}
        value={library.filters.kinds[0] ?? ""}
        ariaLabel={t("File type")}
        width={112}
        onchange={(v: string) => (library.filters.kinds = v ? [v as LibraryItem["kind"]] : [])}
      />
      {#if library.cull.size}
        <Select
          options={[
            { value: "", label: t("Any result") },
            { value: "blurry", label: t("Soft") },
            { value: "exposure", label: t("Bad exposure") },
            { value: "eyes-closed", label: t("Eyes closed") },
            { value: "duplicate", label: t("Similar shots") },
            { value: "best", label: t("Best of group") },
          ]}
          value={library.filters.badge}
          ariaLabel={t("Culling result")}
          width={112}
          testid="lib-filter-badge"
          onchange={(v) => (library.filters.badge = v as typeof library.filters.badge)}
        />
      {/if}
      {#if filtersActive}
        <button type="button" class="oa-btn oa-btn--ghost clear" onclick={() => (library.filters = { ...DEFAULT_FILTERS })}>{t("Clear filters")}</button>
      {/if}
      <span class="grow"></span>
      {#if library.mode === "grid"}
        <LayoutGrid size={12} class="dim" />
        <input class="size" type="range" min="96" max="320" step="8" bind:value={library.settings.thumbSize} aria-label={t("Thumbnail size")} />
      {/if}
    </div>
  {/if}

  <div class="main">
    <div class="stage">
      {#if library.scanning}
        <div class="empty" data-testid="lib-scanning">
          <h2>{t("Reading the folder…")}</h2>
          <p>{t("{n} photos found so far", { n: library.scanning.count })}</p>
        </div>
      {:else if !library.items.length}
        <div class="empty" data-testid="lib-empty">
          <h2>{library.folder ? t("No photos in this folder") : t("Browse a folder of photos")}</h2>
          <p>
            {library.folder
              ? t("It has no JPEG, PNG, WebP, HEIC, TIFF, PSD or raw files. Choose another folder.")
              : t("Rate, flag and compare photos where they are. Nothing is imported or uploaded.")}
          </p>
          <div class="actions">
            <button type="button" class="oa-btn oa-btn--primary oa-btn--md" onclick={openFolder}><FolderOpen size={15} />{t("Open folder")}</button>
            {#if recent && library.supportsPicker && !library.folder}
              <button type="button" class="oa-btn oa-btn--secondary oa-btn--md" onclick={() => library.reopenRecent()}>{t("Reopen {name}", { name: recent })}</button>
            {/if}
          </div>
          {#if !library.supportsPicker}
            <p class="note">{t("This browser can read a folder but not write to it, so ratings stay in this browser and XMP sidecars cannot be saved.")}</p>
          {:else}
            <button type="button" class="link" onclick={() => input.click()}>{t("Or choose a folder without write access")}</button>
          {/if}
        </div>
      {:else if library.mode === "grid"}
        {#if library.view.length}
          <Grid bind:columns onopen={(it) => { library.select(it.path); setMode("loupe"); }} />
        {:else}
          <div class="empty">
            <h2>{t("No photos match these filters")}</h2>
            <p>{t("{n} photos are hidden.", { n: library.items.length })}</p>
            <div class="actions"><button type="button" class="oa-btn oa-btn--secondary" onclick={() => (library.filters = { ...DEFAULT_FILTERS })}>{t("Clear filters")}</button></div>
          </div>
        {/if}
      {:else if library.mode === "loupe"}
        <div class="abs" data-testid="lib-loupe">
          <Viewer item={library.focused} bind:view={loupeView} testid="lib-loupe-viewer" />
        </div>
      {:else if library.mode === "compare"}
        <Compare bind:pair />
      {:else}
        <Survey bind:paths={survey} />
      {/if}
    </div>
    {#if showSidebar && library.items.length}
      <MetaPanel />
    {/if}
  </div>
  {#if library.items.length && library.mode !== "grid"}
    <Filmstrip />
  {/if}

  <footer class="status" data-testid="lib-status">
    {#if library.message}
      <span class="msg" class:error={library.message.kind === "error"} data-testid="lib-message">{library.message.text}</span>
      {#if library.message.undo}
        <button type="button" class="oa-btn oa-btn--ghost small" onclick={() => { library.message?.undo?.(); library.message = null; }}>{t("Undo")}</button>
      {/if}
      <IconButton label={t("Dismiss")} size="xs" onclick={() => (library.message = null)}><X size={11} /></IconButton>
    {:else if library.items.length}
      <span class="msg dim">
        {t("{n} selected", { n: library.selected.size })}{#if library.settings.autoAdvance} · {t("Auto-advance")}{/if}{#if library.settings.writeXmp && library.writable} · {t("Saving XMP sidecars")}{/if}
      </span>
    {/if}
    <span class="grow"></span>
    {#if library.faces.status === "unavailable" && library.cull.size}
      <span class="msg dim" use:tooltip={library.faces.reason}>{t("Face checks unavailable; culling used focus and exposure only")}</span>
    {/if}
    <button type="button" class="hint" onclick={() => (helpOpen = !helpOpen)} aria-expanded={helpOpen}><Keyboard size={12} />{t("Shortcuts")}</button>
  </footer>

  {#if helpOpen}
    <div class="help" role="dialog" aria-label={t("Keyboard shortcuts")} data-testid="lib-help">
      <div class="help-head">
        <span class="ops-section-title">{t("Keyboard shortcuts")}</span>
        <IconButton label={t("Close")} size="xs" onclick={() => (helpOpen = false)}><X size={11} /></IconButton>
      </div>
      <dl>
        {#each shortcuts as [k, v] (k)}
          <dt><span class="lib-kbd">{k}</span></dt>
          <dd>{t(v)}</dd>
        {/each}
      </dl>
    </div>
  {/if}
</div>

{#if settingsMenu}
  <Menu items={settingsItems} anchor={settingsMenu} label={t("Library settings")} onclose={() => (settingsMenu = null)} minWidth={260} />
{/if}

{#if batchOpen}
  <BatchDialog inputs={library.batchInputs()} folder={library.folder?.handle ?? null} onclose={() => (batchOpen = false)} />
{/if}

<style>
  .lib {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-page);
    color: var(--text-body);
  }
  .bar,
  .filters {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border-bottom: var(--border-width) solid var(--border-hairline);
    min-width: 0;
  }
  .bar {
    height: 44px;
    background: var(--bg-subtle);
  }
  .filters {
    height: 36px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .folder {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }
  .fname {
    font: var(--weight-medium) var(--text-sm) / 1 var(--font-sans);
    color: var(--text-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }
  .count {
    font: var(--type-caption);
    color: var(--text-faint);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--border-hairline);
    flex: 0 0 auto;
  }
  .grow {
    flex: 1;
  }
  .cullrun {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .stage {
    font: var(--type-caption);
    color: var(--text-muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .stars,
  .labels {
    display: inline-flex;
    align-items: center;
  }
  .stars button,
  .labels button {
    display: grid;
    place-items: center;
    width: 20px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .stars button.on {
    color: var(--text-strong);
  }
  .stars button:hover,
  .labels button:hover {
    background: var(--surface-hover);
  }
  .labels button.on {
    background: var(--surface-active);
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  .clear {
    height: 24px;
  }
  .size {
    width: 96px;
    accent-color: var(--text-strong);
  }
  .filters :global(.dim) {
    color: var(--text-faint);
    flex: 0 0 auto;
  }
  .main {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
    background: var(--bg-sunken);
  }
  .abs {
    position: absolute;
    inset: 0;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-8);
    text-align: center;
  }
  .empty h2 {
    margin: 0;
    font: var(--type-h3);
    color: var(--text-strong);
  }
  .empty p {
    margin: 0;
    max-width: 440px;
    font: var(--type-body);
    color: var(--text-muted);
  }
  .empty .actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
  .empty .note {
    margin-top: var(--space-3);
    font: var(--type-caption);
    color: var(--text-faint);
  }
  .link {
    margin-top: var(--space-2);
    padding: 0;
    border: 0;
    background: none;
    font: var(--type-caption);
    color: var(--text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .status {
    flex: 0 0 26px;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border-top: var(--border-width) solid var(--border-hairline);
    background: var(--bg-subtle);
    font: var(--type-caption);
    font-size: var(--text-2xs);
    min-width: 0;
  }
  .msg {
    color: var(--text-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .msg.error {
    color: var(--danger-fg);
  }
  .msg.dim {
    color: var(--text-faint);
  }
  .small {
    height: 20px;
    padding: 0 6px;
    font-size: var(--text-2xs);
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px;
    height: 20px;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-faint);
    font: inherit;
    cursor: pointer;
  }
  .hint:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }
  .help {
    position: absolute;
    right: calc(var(--lib-side-w) + var(--space-3));
    bottom: 34px;
    z-index: 20;
    width: 300px;
    padding: var(--space-3);
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    box-shadow: var(--shadow-xl);
  }
  .help-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-2);
  }
  .help dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px var(--space-3);
    margin: 0;
    font: var(--type-caption);
  }
  .help dd {
    margin: 0;
    color: var(--text-body);
  }
</style>
