<script lang="ts">
  // The Layers panel, with Photoshop's controls: blend mode, opacity, locks,
  // fill; rows with visibility, thumbnails, mask thumbnails (click to target
  // the mask), clipping, groups, layer effects and smart filters; drag to
  // reorder or into groups; Shift/Cmd multi-select; double-click to rename.
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Lock from "@lucide/svelte/icons/lock";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import CornerLeftDown from "@lucide/svelte/icons/corner-left-down";
  import Link2 from "@lucide/svelte/icons/link-2";
  import SquareDashed from "@lucide/svelte/icons/square-dashed";
  import Move from "@lucide/svelte/icons/move";
  import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
  import Brush from "@lucide/svelte/icons/brush";
  import Contrast from "@lucide/svelte/icons/contrast";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import SquarePlus from "@lucide/svelte/icons/square-plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Type from "@lucide/svelte/icons/type";
  import FileImage from "@lucide/svelte/icons/file-image";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import { tick } from "svelte";
  import { BLEND_LABELS, BLEND_MODES, type BlendMode, type LayerId } from "../../engine/types";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import IconButton from "../../ui/IconButton.svelte";
  import Menu from "../../ui/Menu.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import Select from "../../ui/Select.svelte";
  import { SEP, tidy, type MenuEntry } from "../../ui/menu";
  import { modHeld } from "../../ui/platform";
  import { paintTarget, setPaintTarget } from "../../ui/paint-target.svelte";
  import { css } from "../../ui/color";
  import { tooltip } from "../../ui/tooltip";
  import { ACTIONS } from "../actions.svelte";
  import { ADJUSTMENT_KINDS, kindInfo } from "../adjustments";
  import { rafThrottle, run } from "../engine.svelte";
  import { filterByOp } from "../filters";
  import * as ops from "../layer-ops";
  import { pro } from "../state.svelte";
  import { EFFECTS, type ProLayer } from "../types";
  import LayerThumb from "./LayerThumb.svelte";
  import { buildRows, isDescendant, type Row } from "./layer-rows";

  const LABEL_COLORS = ["transparent", "rgb(190 70 70 / .55)", "rgb(200 120 50 / .55)", "rgb(190 170 60 / .55)", "rgb(80 160 90 / .55)", "rgb(70 120 190 / .55)", "rgb(130 90 180 / .55)", "rgb(128 128 128 / .55)"];
  const LABEL_NAMES = [t("No colour"), t("Red"), t("Orange"), t("Yellow"), t("Green"), t("Blue"), t("Violet"), t("Gray")];

  const s = $derived(editor.summary);
  const layers = $derived((s?.layers ?? []) as ProLayer[]);
  const active = $derived(editor.active as ProLayer | null);
  let collapsedFx = $state(new Set<LayerId>());
  const rows = $derived(buildRows(layers, collapsedFx, (op) => filterByOp(op)?.label ?? op.replace(/^filter\./, "")));
  const docRatio = $derived(s ? s.width / s.height : 1);
  const selected = $derived(new Set(pro.selection()));

  // Keep the multi-selection honest when the active layer changes elsewhere.
  $effect(() => {
    const a = s?.active;
    if (a != null && !pro.selectedIds.includes(a)) pro.selectedIds = [a];
  });
  // Keep the active layer in view when it changes (new layers, groups, undo).
  $effect(() => {
    const a = s?.active;
    if (a == null) return;
    queueMicrotask(() => list?.querySelector(`[data-layer-id="${a}"]`)?.scrollIntoView({ block: "nearest" }));
  });
  // The mask target belongs to one layer.
  $effect(() => {
    const a = active;
    if (paintTarget.value === "mask" && (!a?.mask || paintTarget.layerId !== a.id)) setPaintTarget("pixels", a?.id ?? null);
  });

  // ---------------------------------------------------------------------------
  // Blend, opacity, fill, locks

  const blendOptions = $derived.by(() => {
    const opts: ({ value: string; label: string } | null)[] = [];
    if (active?.kind === "group") opts.push({ value: "pass-through", label: t("Pass Through") });
    for (const m of BLEND_MODES) opts.push(m === null ? null : { value: m, label: t(BLEND_LABELS[m]) });
    return opts;
  });
  const blendValue = $derived(active?.kind === "group" && active.passThrough ? "pass-through" : (active?.blend ?? "normal"));

  let opacityDraft = $state<number | null>(null);
  let fillDraft = $state<number | null>(null);

  const sendProps = rafThrottle(async (id: LayerId, props: Record<string, unknown>) => {
    await run({ op: "layer.props", id, ...props });
  });

  async function endScrub() {
    await sendProps.flush();
    await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
    opacityDraft = null;
    fillDraft = null;
  }

  async function setBlend(v: string) {
    if (!active) return;
    if (v === "pass-through") await ops.setProps(active.id, { pass_through: true });
    else if (active.kind === "group") await ops.setProps(active.id, { pass_through: false, blend: v as BlendMode });
    else await ops.setProps(active.id, { blend: v as BlendMode });
  }

  function toggleLock(key: "transparency" | "pixels" | "position" | "all") {
    if (!active) return;
    void ops.setProps(active.id, { locks: { ...active.locks, [key]: !active.locks[key] } });
  }

  // ---------------------------------------------------------------------------
  // Selection

  let anchor: LayerId | null = null;
  const layerRows = $derived(rows.filter((r): r is Extract<Row, { type: "layer" }> => r.type === "layer"));

  function selectRow(id: LayerId, e: MouseEvent | KeyboardEvent) {
    if (e.shiftKey && anchor != null) {
      const ids = layerRows.map((r) => r.layer.id);
      const a = ids.indexOf(anchor);
      const b = ids.indexOf(id);
      if (a >= 0 && b >= 0) {
        pro.selectedIds = ids.slice(Math.min(a, b), Math.max(a, b) + 1);
        void ops.setActive(id);
        return;
      }
    }
    if (modHeld(e)) {
      const cur = pro.selection();
      if (cur.includes(id) && cur.length > 1) {
        const next = cur.filter((x) => x !== id);
        pro.selectedIds = next;
        if (s?.active === id) void ops.setActive(next[next.length - 1]);
      } else {
        pro.selectedIds = [...cur.filter((x) => x !== id), id];
        void ops.setActive(id);
      }
      anchor = id;
      return;
    }
    anchor = id;
    pro.selectedIds = [id];
    void ops.setActive(id);
  }

  // ---------------------------------------------------------------------------
  // Rename

  let renaming = $state<LayerId | null>(null);
  let renameText = $state("");

  async function startRename(l: ProLayer) {
    renaming = l.id;
    renameText = l.name;
    await tick();
    const input = document.querySelector<HTMLInputElement>(`[data-rename="${l.id}"]`);
    input?.focus();
    input?.select();
  }

  async function finishRename(commit: boolean) {
    const id = renaming;
    renaming = null;
    if (!commit || id == null) return;
    const name = renameText.trim();
    const l = layerRows.find((r) => r.layer.id === id)?.layer;
    if (name && l && name !== l.name) await ops.setProps(id, { name }, t("Rename layer"));
  }

  let lastRenameRequest = pro.renameRequest;
  $effect(() => {
    if (pro.renameRequest !== lastRenameRequest) {
      lastRenameRequest = pro.renameRequest;
      if (active) void startRename(active);
    }
  });

  // ---------------------------------------------------------------------------
  // Drag and drop

  let list: HTMLDivElement;
  let press: { id: LayerId; x: number; y: number; pointer: number } | null = null;
  let drag = $state<{ id: LayerId; target: Row | null; where: "above" | "below" | "into" | null; y: number } | null>(null);

  function rowDown(e: PointerEvent, r: Row) {
    if (e.button !== 0 || r.type !== "layer") return;
    if ((e.target as HTMLElement).closest("button, input")) return;
    press = { id: r.layer.id, x: e.clientX, y: e.clientY, pointer: e.pointerId };
  }

  function listMove(e: PointerEvent) {
    if (!press) return;
    if (!drag) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 5) return;
      list.setPointerCapture(press.pointer);
      drag = { id: press.id, target: null, where: null, y: 0 };
    }
    const els = [...list.querySelectorAll<HTMLElement>("[data-row]")];
    const lr = list.getBoundingClientRect();
    let target: Row | null = null;
    let where: "above" | "below" | "into" | null = null;
    let y = 0;
    for (const el of els) {
      const r = el.getBoundingClientRect();
      if (e.clientY < r.top || e.clientY >= r.bottom) continue;
      const row = rows.find((x) => x.key === el.dataset.row) ?? null;
      if (!row) break;
      if (row.type !== "layer") {
        // Effects and filters belong to their layer: dropping there means below it.
        const owner = layerRows.find((x) => x.layer.id === row.layer.id) ?? null;
        target = owner;
        where = "below";
        const last = els.filter((x) => rows.find((rr) => rr.key === x.dataset.row)?.layer.id === row.layer.id).at(-1)!.getBoundingClientRect();
        y = last.bottom - lr.top + list.scrollTop;
        break;
      }
      const f = (e.clientY - r.top) / r.height;
      target = row;
      if (row.layer.kind === "group" && f > 0.3 && f < 0.7) where = "into";
      else where = f < 0.5 ? "above" : "below";
      y = (where === "above" ? r.top : r.bottom) - lr.top + list.scrollTop;
      break;
    }
    if (!target && els.length) {
      const last = els[els.length - 1].getBoundingClientRect();
      if (e.clientY >= last.bottom) {
        const bottomRow = layerRows.filter((x) => x.depth === 0).at(-1) ?? null;
        target = bottomRow;
        where = "below";
        y = last.bottom - lr.top + list.scrollTop;
      }
    }
    const dragged = layerRows.find((x) => x.layer.id === drag!.id)?.layer;
    if (target && target.type === "layer" && dragged && (target.layer.id === dragged.id || isDescendant(dragged, target.layer.id))) {
      target = null;
      where = null;
    }
    drag = { id: drag.id, target, where, y };
  }

  async function listUp() {
    const d = drag;
    press = null;
    drag = null;
    if (!d?.target || d.target.type !== "layer" || !d.where) return;
    const tr = d.target;
    let parent: LayerId | null;
    let index: number;
    if (d.where === "into") {
      parent = tr.layer.id;
      index = tr.layer.children?.length ?? 0;
    } else if (d.where === "above") {
      parent = tr.parent;
      index = tr.index + 1;
    } else if (tr.layer.kind === "group" && tr.layer.expanded !== false && tr.layer.children?.length) {
      parent = tr.layer.id;
      index = tr.layer.children.length;
    } else {
      parent = tr.parent;
      index = tr.index;
    }
    await run({ op: "layer.move", id: d.id, ...(parent != null ? { parent } : {}), index }, undefined, t("Move layer"));
  }

  // ---------------------------------------------------------------------------
  // Context menu

  let ctx = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null);

  function act(id: string, label?: string): MenuEntry {
    const a = ACTIONS[id];
    const enabled = a.enabled ? a.enabled() : true;
    return { label: label ?? (typeof a.label === "function" ? a.label() : a.label), shortcut: a.shortcut, disabled: !enabled, run: () => void a.run() };
  }

  let copiedStyle: ProLayer["effects"] | null = null;

  function openContext(e: MouseEvent, r: Row) {
    e.preventDefault();
    if (r.type === "layer" && !selected.has(r.layer.id)) selectRow(r.layer.id, new MouseEvent("click"));
    const l = r.layer;
    let items: MenuEntry[];
    if (r.type === "sf") {
      items = [
        { label: t("Edit Smart Filter…"), run: () => editSmartFilter(l, r.index) },
        { label: t("Blending Options…"), run: () => pro.open({ kind: "smart-filter-blend", id: l.id, index: r.index }) },
        SEP,
        { label: r.enabled ? t("Disable Smart Filter") : t("Enable Smart Filter"), run: () => void ops.setSmartFilter(l.id, r.index, { enabled: !r.enabled }) },
        { label: t("Delete Smart Filter"), run: () => void ops.removeSmartFilter(l.id, r.index) },
      ];
    } else {
      items = tidy([
        { label: t("Blending Options…"), run: () => pro.open({ kind: "layer-style", id: l.id, section: "blending" }) },
        SEP,
        act("layer.duplicate"),
        act("layer.delete"),
        act("layer.rename"),
        SEP,
        act("layer.group", t("Group from Layers")),
        l.kind === "group" ? act("layer.ungroup") : null,
        SEP,
        act("layer.smart-convert"),
        l.kind === "smart" ? act("layer.smart-replace") : null,
        l.kind === "smart" ? act("layer.smart-unpack") : null,
        act("layer.rasterize", t("Rasterize Layer")),
        SEP,
        l.mask ? act("layer.mask-toggle", l.mask.enabled ? t("Disable Layer Mask") : t("Enable Layer Mask")) : act("layer.mask-reveal-all", t("Add Layer Mask")),
        l.mask ? act("layer.mask-delete", t("Delete Layer Mask")) : null,
        l.mask ? act("layer.mask-apply", t("Apply Layer Mask")) : null,
        SEP,
        act("layer.clip"),
        SEP,
        {
          label: t("Copy Layer Style"),
          disabled: !l.effects,
          run: () => (copiedStyle = l.effects ? structuredClone($state.snapshot(l.effects)) : null),
        },
        {
          label: t("Paste Layer Style"),
          disabled: !copiedStyle,
          run: async () => {
            for (const id of pro.selection()) await run({ op: "layer.set-effects", id, effects: copiedStyle });
            await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
          },
        },
        act("layer.style-clear"),
        SEP,
        act("layer.merge"),
        act("layer.merge-visible"),
        act("layer.flatten"),
        SEP,
        {
          label: t("Colour label"),
          submenu: LABEL_NAMES.map((name, i) => ({ label: name, checked: l.colorLabel === i, radio: true, run: () => void ops.setProps(l.id, { color_label: i }) })),
        },
      ]);
    }
    ctx = { x: e.clientX, y: e.clientY, items };
  }

  function editSmartFilter(l: ProLayer, index: number) {
    const f = l.smart?.filters[index];
    if (!f) return;
    const def = filterByOp(f.filter.op);
    if (!def || !def.params.length) {
      pro.open({ kind: "smart-filter-blend", id: l.id, index });
      return;
    }
    pro.open({ kind: "filter", def, smart: { id: l.id, index, initial: f.filter } });
  }

  // ---------------------------------------------------------------------------
  // Keyboard

  function listKey(e: KeyboardEvent) {
    if (renaming != null) return;
    const ids = layerRows.map((r) => r.layer.id);
    const i = s?.active != null ? ids.indexOf(s.active) : -1;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const j = Math.max(0, Math.min(ids.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)));
      if (ids[j] != null) selectRow(ids[j], e);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      e.stopPropagation();
      void ops.deleteLayers();
    } else if (e.key === "F2" || e.key === "Enter") {
      e.preventDefault();
      if (active) void startRename(active);
    } else if ((e.key === "ArrowRight" || e.key === "ArrowLeft") && active?.kind === "group") {
      e.preventDefault();
      void ops.setProps(active.id, { expanded: e.key === "ArrowRight" });
    }
  }

  let adjMenu = $state<DOMRect | null>(null);
  let fxMenu = $state<DOMRect | null>(null);

  const adjItems = $derived<MenuEntry[]>([
    { label: t("Solid Color…"), run: () => pro.open({ kind: "fill-layer", fill: "solid" }) },
    { label: t("Gradient…"), run: () => pro.open({ kind: "fill-layer", fill: "gradient" }) },
    SEP,
    ...ADJUSTMENT_KINDS.map((k): MenuEntry => (k ? { label: `${k.label}…`, icon: k.icon, testid: `add-adjustment-${k.kind}`, run: () => void ops.newAdjustmentLayer(k.kind) } : SEP)),
  ]);
  const fxItems = $derived<MenuEntry[]>([
    { label: t("Blending Options…"), run: () => active && pro.open({ kind: "layer-style", id: active.id, section: "blending" }) },
    SEP,
    ...EFFECTS.map((e): MenuEntry => ({ label: `${t(e.label)}…`, checked: !!active?.effects?.[e.key], run: () => active && pro.open({ kind: "layer-style", id: active.id, section: e.key }) })),
    SEP,
    { label: t("Clear Layer Style"), disabled: !active?.effects, run: () => void ACTIONS["layer.style-clear"].run() },
  ]);

  function kindIcon(l: ProLayer) {
    if (l.kind === "adjustment") return kindInfo(String(l.adjustment?.kind))?.icon ?? SlidersHorizontal;
    if (l.kind === "text") return Type;
    return null;
  }
</script>

<div class="panel" data-testid="layers-panel">
  <div class="controls">
    <div class="ops-row">
      <span class="grow">
        <Select options={blendOptions} value={blendValue} ariaLabel={t("Blend mode")} testid="blend-mode" disabled={!active} onchange={setBlend} />
      </span>
      <NumberField
        label={t("Opacity")}
        value={opacityDraft ?? Math.round((active?.opacity ?? 1) * 100)}
        min={0}
        max={100}
        unit="%"
        width={52}
        disabled={!active}
        testid="layer-opacity"
        oninput={(v) => {
          opacityDraft = v;
          if (active) sendProps(active.id, { opacity: v / 100 });
        }}
        onchange={endScrub}
      />
    </div>
    <div class="ops-row">
      <span class="ops-label">{t("Lock")}</span>
      <div class="locks">
        <IconButton size="xs" label={t("Lock transparent pixels")} pressed={!!active?.locks.transparency} disabled={!active} onclick={() => toggleLock("transparency")}><Grid2x2 size={12} /></IconButton>
        <IconButton size="xs" label={t("Lock image pixels")} pressed={!!active?.locks.pixels} disabled={!active} onclick={() => toggleLock("pixels")}><Brush size={12} /></IconButton>
        <IconButton size="xs" label={t("Lock position")} pressed={!!active?.locks.position} disabled={!active} onclick={() => toggleLock("position")}><Move size={12} /></IconButton>
        <IconButton size="xs" label={t("Lock all")} shortcut="Mod+/" pressed={!!active?.locks.all} disabled={!active} onclick={() => toggleLock("all")}><Lock size={12} /></IconButton>
      </div>
      <span class="grow"></span>
      <NumberField
        label={t("Fill")}
        value={fillDraft ?? Math.round((active?.fillOpacity ?? 1) * 100)}
        min={0}
        max={100}
        unit="%"
        width={52}
        disabled={!active}
        testid="layer-fill"
        oninput={(v) => {
          fillDraft = v;
          if (active) sendProps(active.id, { fill_opacity: v / 100 });
        }}
        onchange={endScrub}
      />
    </div>
  </div>

  <div
    class="list"
    bind:this={list}
    role="tree"
    tabindex="0"
    aria-label={t("Layers")}
    aria-multiselectable="true"
    onpointermove={listMove}
    onpointerup={listUp}
    onpointercancel={() => {
      press = null;
      drag = null;
    }}
    onkeydown={listKey}
  >
    {#each rows as r (r.key)}
      {#if r.type === "layer"}
        {@const l = r.layer}
        {@const Icon = kindIcon(l)}
        <!-- svelte-ignore a11y_click_events_have_key_events (the tree handles keys) -->
        <div
          class="row layer"
          class:selected={selected.has(l.id)}
          class:active={s?.active === l.id}
          class:hidden-layer={!l.visible}
          class:dragging={drag?.id === l.id}
          class:drop-into={drag?.target?.key === r.key && drag.where === "into"}
          data-row={r.key}
          data-testid="layer-row"
          data-layer-id={l.id}
          data-layer-name={l.name}
          data-kind={l.kind}
          role="treeitem"
          tabindex="-1"
          aria-level={r.depth + 1}
          aria-selected={selected.has(l.id)}
          aria-expanded={l.kind === "group" ? l.expanded !== false : undefined}
          style:--depth={r.depth}
          onpointerdown={(e) => rowDown(e, r)}
          onclick={(e) => {
            if ((e.target as HTMLElement).closest("button, input")) return;
            selectRow(l.id, e);
          }}
          ondblclick={(e) => {
            const el = e.target as HTMLElement;
            if (el.closest(".name")) void startRename(l);
            else if (el.closest(".thumbs")) {
              if (l.kind === "adjustment" || l.kind === "text" || l.kind === "shape") pro.showPanel("properties");
              else if (l.kind === "fill") pro.showPanel("properties");
            } else if (!el.closest("button")) pro.open({ kind: "layer-style", id: l.id, section: "blending" });
          }}
          oncontextmenu={(e) => openContext(e, r)}
        >
          <button
            type="button"
            class="eye"
            style:background={LABEL_COLORS[l.colorLabel] ?? "transparent"}
            aria-label={l.visible ? t("Hide {name}", { name: l.name }) : t("Show {name}", { name: l.name })}
            aria-pressed={l.visible}
            data-testid="layer-visibility"
            onclick={() => ops.setProps(l.id, { visible: !l.visible }, t("Visibility"))}
          >
            {#if l.visible}<Eye size={13} />{:else}<EyeOff size={13} />{/if}
          </button>
          <span class="indent"></span>
          {#if r.clipped}<span class="clip" use:tooltip={t("Clipped to the layer below")}><CornerLeftDown size={12} /></span>{/if}
          {#if l.kind === "group"}
            <button
              type="button"
              class="twisty"
              aria-label={l.expanded !== false ? t("Collapse group") : t("Expand group")}
              onclick={() => ops.setProps(l.id, { expanded: l.expanded === false })}
            >
              {#if l.expanded !== false}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
            </button>
          {/if}
          <span class="thumbs">
            {#if l.kind === "group"}
              <span class="kind-thumb folder">{#if l.expanded !== false}<FolderOpen size={16} />{:else}<Folder size={16} />{/if}</span>
            {:else if l.kind === "adjustment" && Icon}
              <span class="kind-thumb chip" class:targeted={s?.active === l.id && paintTarget.value === "pixels" && !!l.mask}><Icon size={14} /></span>
            {:else if l.kind === "fill" && l.fill?.kind === "solid"}
              <span class="kind-thumb solid" style:background={css(l.fill.color)}></span>
            {:else}
              <button
                type="button"
                class="thumb-btn"
                class:targeted={s?.active === l.id && paintTarget.value === "pixels"}
                aria-label={t("Target layer pixels")}
                onclick={(e) => {
                  if (modHeld(e)) {
                    void run({ op: "select.layer-alpha", id: l.id, mode: e.shiftKey ? "add" : "replace" });
                    return;
                  }
                  selectRow(l.id, e);
                  setPaintTarget("pixels", l.id);
                }}
              >
                <LayerThumb id={l.id} rev={l.rev} {docRatio} />
                {#if l.kind === "smart"}<span class="badge" use:tooltip={t("Smart object")}><FileImage size={9} /></span>{/if}
                {#if l.kind === "text"}<span class="badge"><Type size={9} /></span>{/if}
              </button>
            {/if}
            {#if l.mask}
              <span class="chain" class:off={!l.mask.linked}><Link2 size={10} /></span>
              <button
                type="button"
                class="thumb-btn mask"
                class:targeted={s?.active === l.id && paintTarget.value === "mask"}
                class:disabled={!l.mask.enabled}
                aria-label={t("Target layer mask")}
                data-testid="mask-thumb"
                onclick={(e) => {
                  if (e.shiftKey) {
                    void run({ op: "layer.mask-props", id: l.id, enabled: !l.mask!.enabled });
                    return;
                  }
                  selectRow(l.id, e);
                  setPaintTarget("mask", l.id);
                }}
              >
                <LayerThumb id={l.id} rev={l.rev} mask {docRatio} />
              </button>
            {/if}
          </span>
          {#if renaming === l.id}
            <input
              class="ops-field rename"
              data-rename={l.id}
              bind:value={renameText}
              aria-label={t("Layer name")}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") void finishRename(true);
                else if (e.key === "Escape") void finishRename(false);
              }}
              onblur={() => finishRename(true)}
            />
          {:else}
            <span class="name" class:clip-base={r.clipBase}>{l.name}</span>
          {/if}
          <span class="trail">
            {#if l.effects}
              <button
                type="button"
                class="fx"
                class:off={!l.effects.enabled}
                aria-label={collapsedFx.has(l.id) ? t("Show effects") : t("Hide effects list")}
                onclick={() => {
                  const next = new Set(collapsedFx);
                  if (next.has(l.id)) next.delete(l.id);
                  else next.add(l.id);
                  collapsedFx = next;
                }}>fx</button
              >
            {/if}
            {#if l.locks.all || l.locks.pixels || l.locks.position || l.locks.transparency}
              <span class="lock" class:partial={!l.locks.all}><Lock size={11} /></span>
            {/if}
          </span>
        </div>
      {:else if r.type === "fx-header" || r.type === "sf-header"}
        {@const l = r.layer}
        <div class="row sub header" data-row={r.key} role="treeitem" tabindex="-1" aria-selected="false" style:--depth={r.depth} oncontextmenu={(e) => e.preventDefault()}>
          {#if r.type === "fx-header"}
            <button
              type="button"
              class="eye"
              aria-label={l.effects?.enabled ? t("Hide all effects") : t("Show all effects")}
              onclick={() => run({ op: "layer.set-effects", id: l.id, effects: { ...$state.snapshot(l.effects), enabled: !l.effects?.enabled } })}
            >
              {#if l.effects?.enabled}<Eye size={12} />{:else}<EyeOff size={12} />{/if}
            </button>
            <span class="sub-label">{t("Effects")}</span>
          {:else}
            <span class="eye"></span>
            <span class="sub-label">{t("Smart Filters")}</span>
          {/if}
        </div>
      {:else if r.type === "fx"}
        {@const l = r.layer}
        <div
          class="row sub"
          class:off={!r.enabled}
          data-row={r.key}
          role="treeitem"
          tabindex="-1"
          aria-selected="false"
          style:--depth={r.depth}
          ondblclick={() => pro.open({ kind: "layer-style", id: l.id, section: r.effect })}
          oncontextmenu={(e) => e.preventDefault()}
        >
          <button
            type="button"
            class="eye"
            aria-label={r.enabled ? t("Hide {name}", { name: r.label }) : t("Show {name}", { name: r.label })}
            data-testid="effect-visibility"
            onclick={async () => {
              const fx = $state.snapshot(l.effects)!;
              const cur = fx[r.effect];
              if (!cur) return;
              await run({ op: "layer.set-effects", id: l.id, effects: { ...fx, [r.effect]: { ...cur, enabled: !cur.enabled } } });
              await editor.engine.exec({ op: "edit.seal" }).catch(() => null);
            }}
          >
            {#if r.enabled}<Eye size={12} />{:else}<EyeOff size={12} />{/if}
          </button>
          <span class="sub-label">{t(r.label)}</span>
        </div>
      {:else if r.type === "sf"}
        {@const l = r.layer}
        <div
          class="row sub"
          class:off={!r.enabled}
          data-row={r.key}
          data-testid="smart-filter-row"
          role="treeitem"
          tabindex="-1"
          aria-selected="false"
          style:--depth={r.depth}
          ondblclick={() => editSmartFilter(l, r.index)}
          oncontextmenu={(e) => openContext(e, r)}
        >
          <button
            type="button"
            class="eye"
            aria-label={r.enabled ? t("Hide {name}", { name: r.label }) : t("Show {name}", { name: r.label })}
            onclick={() => ops.setSmartFilter(l.id, r.index, { enabled: !r.enabled })}
          >
            {#if r.enabled}<Eye size={12} />{:else}<EyeOff size={12} />{/if}
          </button>
          <span class="sub-label">{r.label}</span>
          <button type="button" class="blend-opts" aria-label={t("Blending options for {name}", { name: r.label })} use:tooltip={t("Blending options")} onclick={() => pro.open({ kind: "smart-filter-blend", id: l.id, index: r.index })}>
            <SlidersHorizontal size={11} />
          </button>
        </div>
      {/if}
    {/each}
    {#if drag?.target && drag.where && drag.where !== "into"}
      <div class="drop-line" style:top="{drag.y - 1}px" style:left="{24 + ((drag.target.type === 'layer' ? drag.target.depth : 0) * 14)}px"></div>
    {/if}
    {#if !rows.length}
      <p class="ops-note empty">{t("Open or create a document to see its layers.")}</p>
    {/if}
  </div>

  <div class="bar">
    <IconButton size="sm" label={t("Link layers")} disabled onclick={() => undefined}><Link2 size={14} /></IconButton>
    <IconButton
      size="sm"
      label={t("Add a layer style")}
      disabled={!active || active.kind === "adjustment"}
      testid="fx-button"
      onclick={(e) => (fxMenu = (e.currentTarget as HTMLElement).getBoundingClientRect())}
    >
      <span class="fx-label">fx</span>
    </IconButton>
    <IconButton size="sm" label={t("Add layer mask")} disabled={!active || !!active.mask} testid="add-mask" onclick={() => ops.addMask(s?.selection ? "selection" : "reveal-all")}>
      <SquareDashed size={14} />
    </IconButton>
    <IconButton
      size="sm"
      label={t("Create new fill or adjustment layer")}
      disabled={!editor.hasDocument}
      testid="add-adjustment"
      onclick={(e) => (adjMenu = (e.currentTarget as HTMLElement).getBoundingClientRect())}
    >
      <Contrast size={14} />
    </IconButton>
    <IconButton size="sm" label={t("Create a new group")} disabled={!editor.hasDocument} testid="new-group" onclick={() => (pro.selection().length > 1 ? ops.groupLayers() : ops.newGroup())}>
      <FolderPlus size={14} />
    </IconButton>
    <IconButton size="sm" label={t("Create a new layer")} shortcut="Shift+Mod+N" disabled={!editor.hasDocument} testid="new-layer" onclick={ops.newLayer}>
      <SquarePlus size={14} />
    </IconButton>
    <IconButton size="sm" label={t("Delete layer")} disabled={!active} testid="delete-layer" onclick={() => ops.deleteLayers()}>
      <Trash2 size={14} />
    </IconButton>
  </div>
</div>

{#if ctx}
  <Menu items={ctx.items} x={ctx.x} y={ctx.y} label={t("Layer")} onclose={() => (ctx = null)} />
{/if}
{#if adjMenu}
  <Menu items={adjItems} anchor={adjMenu} label={t("New fill or adjustment layer")} testid="adjustment-menu" onclose={() => (adjMenu = null)} />
{/if}
{#if fxMenu}
  <Menu items={fxItems} anchor={fxMenu} label={t("Layer style")} onclose={() => (fxMenu = null)} />
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-bottom: var(--border-width) solid var(--border-hairline);
    flex: 0 0 auto;
  }
  .locks {
    display: flex;
    gap: 1px;
  }
  .list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    outline: none;
    user-select: none;
  }
  .list:focus-visible {
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    height: 36px;
    padding-right: 6px;
    border-bottom: var(--border-width) solid var(--border-hairline);
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-body);
    cursor: default;
  }
  .row.layer:hover {
    background: var(--surface-hover);
  }
  .row.selected {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .row.selected.active {
    background: color-mix(in oklab, var(--text-strong) 16%, transparent);
  }
  .row.hidden-layer .thumbs,
  .row.hidden-layer .name {
    opacity: 0.55;
  }
  .row.dragging {
    opacity: 0.5;
  }
  .row.drop-into {
    box-shadow: inset 0 0 0 1px var(--text-strong);
  }
  .row.sub {
    height: 22px;
    color: var(--text-muted);
    border-bottom: 0;
  }
  .row.sub.off .sub-label {
    opacity: 0.5;
    text-decoration: line-through;
  }
  .eye {
    display: grid;
    place-items: center;
    width: 26px;
    height: 100%;
    flex: 0 0 auto;
    padding: 0;
    border: 0;
    border-right: var(--border-width) solid var(--border-hairline);
    background: transparent;
    color: var(--text-muted);
    cursor: default;
  }
  .row.sub .eye {
    border-right-color: transparent;
  }
  .eye:hover {
    color: var(--text-strong);
  }
  .indent {
    width: calc(var(--depth) * 14px + 4px);
    flex: 0 0 auto;
  }
  .row.sub .sub-label {
    margin-left: calc(var(--depth) * 14px + 20px);
  }
  .clip {
    display: grid;
    place-items: center;
    width: 14px;
    margin-left: 6px;
    color: var(--text-muted);
    transform: scaleX(-1);
  }
  .twisty {
    display: grid;
    place-items: center;
    width: 14px;
    height: 20px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: default;
  }
  .thumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 0 2px;
  }
  .thumb-btn {
    position: relative;
    display: grid;
    place-items: center;
    padding: 1px;
    border: 1px solid transparent;
    border-radius: 2px;
    background: var(--bg-sunken);
    cursor: default;
  }
  .thumb-btn.targeted {
    border-color: var(--text-strong);
  }
  .thumb-btn.mask.disabled::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to top right, transparent calc(50% - 1px), var(--danger-fg) 50%, transparent calc(50% + 1px));
  }
  .kind-thumb {
    display: grid;
    place-items: center;
    width: 38px;
    height: 30px;
    color: var(--text-muted);
  }
  .kind-thumb.chip {
    width: 30px;
    margin: 0 4px;
    border-radius: var(--radius-xs);
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .kind-thumb.solid {
    width: 30px;
    height: 26px;
    margin: 0 4px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
  }
  .badge {
    position: absolute;
    right: -1px;
    bottom: -1px;
    display: grid;
    place-items: center;
    width: 12px;
    height: 12px;
    border-radius: 2px;
    background: var(--surface-inverse);
    color: var(--text-inverse);
  }
  .chain {
    display: grid;
    place-items: center;
    width: 10px;
    color: var(--text-faint);
  }
  .chain.off {
    opacity: 0.25;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name.clip-base {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .rename {
    flex: 1;
    min-width: 0;
    height: 22px;
  }
  .trail {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: 4px;
    color: var(--text-muted);
  }
  .fx {
    padding: 0 3px;
    height: 16px;
    border: 0;
    border-radius: 2px;
    background: transparent;
    font: italic var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-muted);
    cursor: default;
  }
  .fx:hover {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .fx.off {
    opacity: 0.5;
  }
  .lock.partial {
    opacity: 0.55;
  }
  .sub-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header .sub-label {
    color: var(--text-muted);
  }
  .blend-opts {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-faint);
    cursor: default;
  }
  .blend-opts:hover {
    color: var(--text-strong);
  }
  .drop-line {
    position: absolute;
    right: 4px;
    height: 2px;
    background: var(--text-strong);
    pointer-events: none;
  }
  .empty {
    padding: var(--space-4);
    text-align: center;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    height: 30px;
    padding: 0 6px;
    border-top: var(--border-width) solid var(--border-hairline);
    flex: 0 0 auto;
  }
  .fx-label {
    font: italic var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
  }
</style>
