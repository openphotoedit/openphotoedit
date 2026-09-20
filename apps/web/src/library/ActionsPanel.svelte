<script lang="ts">
  // The Actions panel: pick an action, record what you do in the editor,
  // add steps by hand, turn steps on and off, and play it on the open photo
  // or hand it to a batch.
  //
  //   <ActionsPanel onbatch={(id) => openBatchWith(id)} />
  import { onMount } from "svelte";
  import Circle from "@lucide/svelte/icons/circle";
  import Play from "@lucide/svelte/icons/play";
  import Square from "@lucide/svelte/icons/square";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Copy from "@lucide/svelte/icons/copy";
  import Upload from "@lucide/svelte/icons/upload";
  import Download from "@lucide/svelte/icons/download";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Hand from "@lucide/svelte/icons/hand";
  import Info from "@lucide/svelte/icons/info";
  import FolderOutput from "@lucide/svelte/icons/folder-output";
  import Pencil from "@lucide/svelte/icons/pencil";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import Dialog from "../ui/Dialog.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Menu from "../ui/Menu.svelte";
  import Select from "../ui/Select.svelte";
  import type { MenuEntry } from "../ui/menu";
  import { tooltip } from "../ui/tooltip";
  import { actions, describeCommand, STEP_TEMPLATES, type ActionStep } from "./actions.svelte";
  import "./library.css";

  let { onbatch }: { onbatch?: (actionId: string) => void } = $props();

  let addMenu = $state<DOMRect | null>(null);
  let addBtn = $state<HTMLElement | null>(null);
  let custom = $state<{ json: string; error: string | null } | null>(null);
  let stopEdit = $state<{ message: string } | null>(null);
  let renaming = $state<string | null>(null);
  let stopPrompt = $state<{ message: string; resolve: (go: boolean) => void } | null>(null);
  let result = $state<{ text: string; error?: boolean } | null>(null);
  let fileInput: HTMLInputElement;

  onMount(() => {
    if (!actions.loaded) void actions.load();
    return () => actions.stopRecording();
  });

  const set = $derived(actions.active);
  const canPlay = $derived(!!set && editor.hasDocument && !actions.playing && !!set.steps.some((s) => s.enabled));

  const addItems = $derived<MenuEntry[]>([
    { type: "heading", label: t("Common steps") },
    ...STEP_TEMPLATES.map((tpl) => ({ label: t(tpl.label), testid: `lib-step-tpl-${tpl.cmd.op}`, run: () => set && actions.addStep(set.id, structuredClone(tpl.cmd), t(tpl.label)) })),
    { type: "separator" },
    { label: t("Stop with a message…"), testid: "lib-step-stop", run: () => (stopEdit = { message: "" }) },
    { label: t("Command as JSON…"), testid: "lib-step-json", run: () => (custom = { json: '{ "op": "filter.gaussian-blur", "radius": 2 }', error: null }) },
  ]);

  async function play() {
    if (!set) return;
    result = null;
    try {
      const r = await actions.play(set.id, actions.editorTarget(), {
        onStop: (message) => new Promise((resolve) => (stopPrompt = { message, resolve })),
      });
      editor.fit?.();
      result = r.stoppedAt != null ? { text: t("Stopped at step {n}.", { n: r.stoppedAt + 1 }) } : { text: r.notes.length ? t("Played {n} steps. {notes}", { n: r.ran, notes: r.notes.join(" ") }) : t("Played {n} steps.", { n: r.ran }) };
    } catch (e) {
      result = { text: e instanceof Error ? e.message : String(e), error: true };
    }
  }

  function addCustom() {
    if (!custom || !set) return;
    try {
      const cmd = JSON.parse(custom.json);
      if (!cmd || typeof cmd.op !== "string") throw new Error(t('A command needs an "op", like "filter.gaussian-blur".'));
      actions.addStep(set.id, cmd, describeCommand(cmd));
      custom = null;
    } catch (e) {
      custom = { ...custom, error: e instanceof Error ? e.message : String(e) };
    }
  }

  function exportFile() {
    if (!set) return;
    const blob = new Blob([actions.exportJson([set.id])], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${set.name.replace(/[\\/:*?"<>|]/g, "_")}.actions.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 5000);
  }

  async function importFile() {
    const f = fileInput.files?.[0];
    fileInput.value = "";
    if (!f) return;
    try {
      const n = await actions.importJson(await f.text());
      result = { text: t("Imported {n} actions.", { n }) };
    } catch (e) {
      result = { text: e instanceof Error ? e.message : String(e), error: true };
    }
  }

  function params(st: ActionStep) {
    if (!st.cmd) return "";
    const { op: _op, ...rest } = st.cmd;
    const s = JSON.stringify(rest, (_k, v) => (v && typeof v === "object" && "$layer" in v ? `‹${v.$layer.name ?? "created layer"}›` : v));
    return s === "{}" ? "" : s.slice(1, -1).replace(/"/g, "").replace(/,/g, ", ").replace(/:/g, ": ");
  }
</script>

<div class="panel lib-root oa-dense" data-testid="lib-actions">
  <input bind:this={fileInput} type="file" accept=".json,application/json" class="visually-hidden" onchange={importFile} data-testid="lib-actions-import-input" tabindex="-1" aria-hidden="true" />
  <header class="head">
    {#if renaming != null && set}
      <input
        class="ops-field grow"
        value={renaming}
        aria-label={t("Action name")}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            actions.rename(set.id, (e.currentTarget as HTMLInputElement).value);
            renaming = null;
          } else if (e.key === "Escape") renaming = null;
        }}
        onblur={(e) => {
          actions.rename(set.id, (e.currentTarget as HTMLInputElement).value);
          renaming = null;
        }}
      />
    {:else}
      <div class="grow sel">
        <Select options={actions.sets.map((s) => ({ value: s.id, label: s.name }))} value={actions.activeId ?? ""} ariaLabel={t("Action")} testid="lib-actions-select" onchange={(v) => (actions.activeId = v)} />
      </div>
      <IconButton label={t("Rename")} size="sm" disabled={!set} onclick={() => (renaming = set?.name ?? "")}><Pencil size={12} /></IconButton>
    {/if}
    <IconButton label={t("New action")} size="sm" onclick={() => actions.newSet()} testid="lib-actions-new"><Plus size={13} /></IconButton>
    <IconButton label={t("Duplicate")} size="sm" disabled={!set} onclick={() => set && actions.duplicate(set.id)}><Copy size={12} /></IconButton>
    <IconButton label={t("Delete action")} size="sm" disabled={!set} onclick={() => set && actions.remove(set.id)}><Trash2 size={12} /></IconButton>
  </header>

  <div class="transport">
    {#if actions.recording}
      <button type="button" class="oa-btn oa-btn--secondary rec on" onclick={() => actions.stopRecording()} data-testid="lib-actions-stop-rec"><Square size={11} fill="currentColor" />{t("Stop recording")}</button>
    {:else}
      <button type="button" class="oa-btn oa-btn--secondary rec" disabled={!set} onclick={() => actions.startRecording()} data-testid="lib-actions-record" use:tooltip={t("Adds each edit you make in the editor as a step")}><Circle size={11} fill="currentColor" />{t("Record")}</button>
    {/if}
    <button type="button" class="oa-btn oa-btn--primary" disabled={!canPlay} onclick={play} data-testid="lib-actions-play" use:tooltip={editor.hasDocument ? t("Play on the open photo") : t("Open a photo to play an action on it")}>
      <Play size={12} fill="currentColor" />{actions.playing ? t("Playing {i}/{n}", { i: actions.playing.index + 1, n: actions.playing.total }) : t("Play")}
    </button>
    <span class="grow"></span>
    {#if onbatch}
      <IconButton label={t("Run in a batch export")} size="sm" disabled={!set} onclick={() => set && onbatch?.(set.id)} testid="lib-actions-batch"><FolderOutput size={13} /></IconButton>
    {/if}
    <IconButton label={t("Import actions")} size="sm" onclick={() => fileInput.click()}><Upload size={13} /></IconButton>
    <IconButton label={t("Export this action")} size="sm" disabled={!set} onclick={exportFile} testid="lib-actions-export"><Download size={13} /></IconButton>
  </div>

  {#if actions.recording}
    <div class="recbar" role="status"><span class="dot"></span>{t("Recording. Edits you make are added below.")}</div>
  {/if}

  <ol class="steps lib-scroll" data-testid="lib-steps">
    {#each set?.steps ?? [] as st, i (st.id)}
      <li class="step" class:off={!st.enabled} class:current={actions.playing?.index === i} data-testid="lib-step" data-op={st.cmd?.op ?? st.kind}>
        <input type="checkbox" class="oa-checkbox" checked={st.enabled} disabled={!st.cmd && st.kind === "command"} onchange={(e) => set && actions.updateStep(set.id, st.id, { enabled: (e.currentTarget as HTMLInputElement).checked })} aria-label={t("Include step {n}", { n: i + 1 })} />
        <span class="num">{i + 1}</span>
        <div class="body">
          <div class="label">
            {#if st.kind === "stop"}<Hand size={11} />{/if}
            {st.label}
            {#if st.note}<span class="note" use:tooltip={st.note}><Info size={11} /></span>{/if}
          </div>
          {#if st.kind === "stop"}
            <div class="params">{st.message}</div>
          {:else if params(st)}
            <div class="params">{params(st)}</div>
          {/if}
        </div>
        <div class="tools">
          <IconButton label={t("Move up")} size="xs" disabled={i === 0} onclick={() => set && actions.moveStep(set.id, st.id, -1)}><ChevronUp size={11} /></IconButton>
          <IconButton label={t("Move down")} size="xs" disabled={i === (set?.steps.length ?? 0) - 1} onclick={() => set && actions.moveStep(set.id, st.id, 1)}><ChevronDown size={11} /></IconButton>
          <IconButton label={t("Delete step")} size="xs" onclick={() => set && actions.removeStep(set.id, st.id)}><Trash2 size={11} /></IconButton>
        </div>
      </li>
    {:else}
      <li class="oa-empty empty">{set ? t("No steps yet. Record your edits, or add steps by hand.") : t("Create an action to get started.")}</li>
    {/each}
  </ol>

  <footer class="foot">
    <span bind:this={addBtn}>
      <button type="button" class="oa-btn oa-btn--ghost" disabled={!set} onclick={() => (addMenu = addBtn!.getBoundingClientRect())} data-testid="lib-actions-add"><Plus size={13} />{t("Add step")}</button>
    </span>
    {#if result}
      <span class="result" class:error={result.error} data-testid="lib-actions-result">{result.text}</span>
    {/if}
  </footer>
</div>

{#if addMenu}
  <Menu items={addItems} anchor={addMenu} label={t("Add step")} onclose={() => (addMenu = null)} minWidth={220} />
{/if}

{#if custom}
  <Dialog title={t("Add a command")} width={420} onclose={() => (custom = null)}>
    <p class="hintp">{t("Any command from the engine's command list. Layer ids are resolved when the action plays.")}</p>
    <textarea class="ops-field json" rows="6" bind:value={custom.json} aria-label={t("Command JSON")} data-testid="lib-step-json-text"></textarea>
    {#if custom.error}<p class="error" role="alert">{custom.error}</p>{/if}
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--ghost" onclick={() => (custom = null)}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" onclick={addCustom} data-testid="lib-step-json-add">{t("Add step")}</button>
    {/snippet}
  </Dialog>
{/if}

{#if stopEdit}
  <Dialog title={t("Add a stop")} width={380} onclose={() => (stopEdit = null)} onsubmit={() => { if (set && stopEdit) actions.addStop(set.id, stopEdit.message); stopEdit = null; }}>
    <label class="hintp" for="lib-stop-msg">{t("Playback pauses here and shows this message.")}</label>
    <input id="lib-stop-msg" class="ops-field wide" bind:value={stopEdit.message} placeholder={t("Check the crop before continuing.")} data-testid="lib-step-stop-text" />
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--ghost" onclick={() => (stopEdit = null)}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" onclick={() => { if (set && stopEdit) actions.addStop(set.id, stopEdit.message); stopEdit = null; }} data-testid="lib-step-stop-add">{t("Add stop")}</button>
    {/snippet}
  </Dialog>
{/if}

{#if stopPrompt}
  <Dialog title={t("Action paused")} width={380} testid="lib-stop-prompt" onclose={() => { stopPrompt?.resolve(false); stopPrompt = null; }}>
    <p class="hintp">{stopPrompt.message}</p>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--ghost" onclick={() => { stopPrompt?.resolve(false); stopPrompt = null; }}>{t("Stop")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-autofocus onclick={() => { stopPrompt?.resolve(true); stopPrompt = null; }} data-testid="lib-stop-continue">{t("Continue")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-subtle);
    color: var(--text-body);
    font: var(--type-caption);
  }
  .head,
  .transport,
  .foot {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px var(--space-2);
  }
  .head {
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .foot {
    border-top: var(--border-width) solid var(--border-hairline);
    min-width: 0;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .sel :global(.ops-select) {
    width: 100%;
  }
  .rec :global(svg) {
    color: var(--danger-fg);
  }
  .rec.on {
    border-color: var(--danger-fg);
  }
  .recbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 var(--space-2) 4px;
    padding: 4px 8px;
    border-radius: var(--radius-xs);
    background: var(--danger-bg);
    color: var(--danger-fg);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--radius-full);
    background: currentColor;
    animation: blink 1.2s var(--ease-standard) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  .steps {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .step {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 5px 4px 5px 6px;
    border-radius: var(--radius-xs);
    border: var(--border-width) solid transparent;
  }
  .step:hover {
    background: var(--surface-hover);
  }
  .step.current {
    border-color: var(--border-focus);
  }
  .step.off .body {
    opacity: 0.5;
  }
  .step .oa-checkbox {
    width: 13px;
    height: 13px;
    margin: 2px 0 0;
  }
  .num {
    width: 14px;
    text-align: right;
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    padding-top: 2px;
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-strong);
  }
  .note {
    display: inline-grid;
    color: var(--warning-fg);
  }
  .params {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tools {
    display: flex;
    opacity: 0;
  }
  .step:hover .tools,
  .step:focus-within .tools {
    opacity: 1;
  }
  .empty {
    padding: var(--space-4) var(--space-2);
    text-align: center;
  }
  .result {
    flex: 1;
    min-width: 0;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .result.error {
    color: var(--danger-fg);
    white-space: normal;
  }
  .hintp {
    display: block;
    margin: 0 0 var(--space-2);
    color: var(--text-muted);
  }
  .json,
  .wide {
    width: 100%;
    box-sizing: border-box;
  }
  .json {
    height: auto;
    padding: 6px;
    font-family: var(--font-mono);
    resize: vertical;
  }
  .error {
    margin: var(--space-2) 0 0;
    color: var(--danger-fg);
  }
</style>
