// Text: click for point text, drag for a paragraph box, click an existing
// text layer to edit it. Typing happens in a positioned textarea
// (TextEditorOverlay) styled from the text options; committing rasterises
// with lib/text.ts and sends `layer.add-text` / `layer.set-text`.

import { mount, unmount } from "svelte";
import type { EditorStore } from "../lib/editor.svelte";
import type { LayerInfo, TextData } from "../engine/types";
import { renderText, textBox } from "../lib/text";
import type { Tool } from "./types";
import { textColor, toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import { antsStroke, docRectPath, inRect, layersTopDown, rectFrom, redraw, rgba, run, setCursor, trackHover, type Pt, setActive } from "./common";
import TextEditorOverlay from "./TextEditorOverlay.svelte";

export interface TextEditSession {
  /** Layer being edited, or null for a new text. */
  layerId: number | null;
  text: string;
  x: number;
  y: number;
  boxWidth: number | null;
  rotation: number;
  /** The layer was hidden with an undoable step that must be reverted. */
  hidden: boolean;
}

export const textEdit = $state<{ session: TextEditSession | null }>({ session: null });

let component: ReturnType<typeof mount> | null = null;
let drag: { start: Pt; end: Pt; startV: Pt; moved: boolean } | null = null;
let committing = false;

/** The text options as `TextData` for the current session. */
export function sessionData(s: TextEditSession): TextData {
  const o = toolSettings;
  return {
    text: s.text,
    font_family: o.fontFamily,
    font_size: o.fontSize,
    font_weight: o.fontWeight,
    italic: o.italic,
    color: rgba(textColor()),
    align: o.textAlign,
    line_height: o.lineHeight,
    letter_spacing: o.letterSpacing,
    box_width: s.boxWidth,
    x: s.x,
    y: s.y,
    rotation: s.rotation,
    background: o.textBackgroundOn ? rgba(o.textBackground) : null,
    padding: o.textBackgroundOn ? o.textPadding : 0,
    stroke: o.textStrokeOn ? rgba(o.textStroke) : null,
    stroke_width: o.textStrokeOn ? o.textStrokeWidth : 0,
  };
}

function loadOptions(d: TextData) {
  const o = toolSettings;
  o.fontFamily = d.font_family;
  o.fontSize = d.font_size;
  o.fontWeight = d.font_weight;
  o.italic = d.italic;
  o.textColor = d.color;
  o.textAlign = d.align;
  o.lineHeight = d.line_height;
  o.letterSpacing = d.letter_spacing;
  o.textBackgroundOn = !!d.background;
  if (d.background) {
    o.textBackground = d.background;
    o.textPadding = d.padding;
  }
  o.textStrokeOn = !!d.stroke && d.stroke_width > 0;
  if (d.stroke) {
    o.textStroke = d.stroke;
    o.textStrokeWidth = d.stroke_width;
  }
}

function openEditor(ed: EditorStore, session: TextEditSession) {
  textEdit.session = session;
  toolState.textEditing = true;
  if (!component) component = mount(TextEditorOverlay, { target: document.body, props: { ed } });
  redraw(ed);
}

function closeEditor(ed: EditorStore) {
  textEdit.session = null;
  toolState.textEditing = false;
  if (component) {
    const c = component;
    component = null;
    void unmount(c);
  }
  redraw(ed);
}

async function unhide(ed: EditorStore, s: TextEditSession) {
  if (!s.hidden || s.layerId == null) return;
  s.hidden = false;
  const undo = ed.summary?.history.undo ?? [];
  if (undo[undo.length - 1] === "Hide Layer") await run(ed, { op: "edit.undo" }, { quiet: true });
  else await run(ed, { op: "layer.props", id: s.layerId, visible: true }, { quiet: true });
}

/** Commit the text being edited (no-op when nothing is open). */
export async function commitText(ed: EditorStore) {
  const s = textEdit.session;
  if (!s || committing) return;
  committing = true;
  try {
    const data = sessionData(s);
    closeEditor(ed);
    await unhide(ed, s);
    if (!data.text.trim()) {
      if (s.layerId != null) await run(ed, { op: "layer.delete", ids: [s.layerId] });
      return;
    }
    const r = await renderText(data);
    const cmd = { data, width: r.width, height: r.height, x: r.x, y: r.y };
    if (s.layerId != null) await run(ed, { op: "layer.set-text", id: s.layerId, ...cmd }, { bytes: r.rgba });
    else await run(ed, { op: "layer.add-text", ...cmd }, { bytes: r.rgba });
  } finally {
    committing = false;
    redraw(ed);
  }
}

export async function cancelText(ed: EditorStore) {
  const s = textEdit.session;
  if (!s) return;
  closeEditor(ed);
  await unhide(ed, s);
}

async function editLayer(ed: EditorStore, l: LayerInfo) {
  const d = l.text!;
  loadOptions(d);
  const session: TextEditSession = { layerId: l.id, text: d.text, x: d.x, y: d.y, boxWidth: d.box_width, rotation: d.rotation, hidden: false };
  await setActive(ed, l.id);
  const r = await run(ed, { op: "layer.props", id: l.id, visible: false }, { quiet: true });
  session.hidden = !!r?.changed;
  openEditor(ed, session);
}

function textLayerAt(ed: EditorStore, p: Pt): LayerInfo | null {
  for (const l of layersTopDown(ed)) {
    if (!l.visible || l.kind !== "text" || !l.text || l.locks.all) continue;
    const b = l.bounds ?? textBox(l.text);
    if (inRect(b, p)) return l;
  }
  return null;
}

export const text: Tool = {
  id: "text",
  label: "Text",
  shortcut: "t",
  cursor: "text",
  deactivate(ed) {
    void commitText(ed);
  },
  down(ed, p) {
    if (textEdit.session) {
      void commitText(ed);
      drag = null;
      return;
    }
    drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, startV: { x: p.vx, y: p.vy }, moved: false };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) {
      if (!textEdit.session) setCursor(textLayerAt(ed, p) ? "text" : "text");
      return;
    }
    drag.end = { x: p.x, y: p.y };
    if (Math.hypot(p.vx - drag.startV.x, p.vy - drag.startV.y) > 5) drag.moved = true;
  },
  async up(ed) {
    const d = drag;
    drag = null;
    if (!d) return;
    if (!d.moved) {
      const hit = textLayerAt(ed, d.start);
      if (hit) {
        await editLayer(ed, hit);
        return;
      }
      // Point text: the click marks the top-left of the first line.
      openEditor(ed, { layerId: null, text: "", x: Math.round(d.start.x), y: Math.round(d.start.y - toolSettings.fontSize * 0.2), boxWidth: null, rotation: 0, hidden: false });
      return;
    }
    const r = rectFrom(d.start, d.end);
    openEditor(ed, { layerId: null, text: "", x: Math.round(r.x), y: Math.round(r.y), boxWidth: Math.max(8, Math.round(r.w)), rotation: 0, hidden: false });
  },
  cancel(ed) {
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (drag?.moved) {
      const r = rectFrom(drag.start, drag.end);
      antsStroke(ctx, () => docRectPath(ed, ctx, r));
    }
  },
};
