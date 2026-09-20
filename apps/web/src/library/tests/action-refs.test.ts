import { describe, expect, it } from "vitest";
import type { LayerInfo, Summary } from "../../engine/types";
import { createdId, isReplayable, parameterize, resolve } from "../action-refs";

function layer(id: number, name: string, kind: LayerInfo["kind"] = "pixel"): LayerInfo {
  return { id, name, kind, visible: true, opacity: 1, fillOpacity: 1, blend: "normal", clip: false, locks: { transparency: false, pixels: false, position: false, all: false }, colorLabel: 0, rev: 0, provenance: [], mask: null };
}
function doc(layers: LayerInfo[], active: number | null): Summary {
  return { width: 100, height: 100, resolution: 72, active, layers, selection: null, history: { undo: [], redo: [] }, revision: 0, source: null };
}

describe("action layer references", () => {
  it("records ids by name and position, and replays onto a document with different ids", () => {
    const rec = doc([layer(1, "Background"), layer(5, "Retouch"), layer(9, "Sky", "adjustment")], 5);
    const cmd = parameterize({ op: "layer.props", id: 5, opacity: 0.5 }, rec, new Map());
    expect(cmd.id).toEqual({ $layer: { name: "Retouch", kind: "pixel", fromTop: 1, active: true } });
    const play = doc([layer(40, "Background"), layer(41, "Sky", "adjustment"), layer(42, "Retouch")], 40);
    const r = resolve(cmd, play, new Map());
    expect(r.cmd).toEqual({ op: "layer.props", id: 42, opacity: 0.5 });
    expect(r.notes).toEqual([]);
  });

  it("maps layers created by earlier steps to the ids created during playback", () => {
    const created = new Map<number, number>([[77, 0]]);
    const cmd = parameterize({ op: "layer.set-adjustment", id: 77, adjustment: { kind: "exposure", exposure: 1 } }, doc([layer(1, "Background"), layer(77, "Exposure 1", "adjustment")], 77), created);
    expect(cmd.id).toEqual({ $layer: { step: 0 } });
    const r = resolve(cmd, doc([layer(3, "Background"), layer(12, "Exposure 1", "adjustment")], 3), new Map([[0, 12]]));
    expect(r.cmd.id).toBe(12);
    expect(createdId({ data: { id: 12 } })).toBe(12);
    expect(createdId({ data: null })).toBeUndefined();
  });

  it("maps id lists and falls back to position, then the active layer, with notes", () => {
    const rec = doc([layer(1, "Background"), layer(2, "Layer 1"), layer(3, "Layer 2")], 3);
    const cmd = parameterize({ op: "layer.delete", ids: [2, 3], name: "keep" }, rec, new Map());
    const play = doc([layer(10, "Photo"), layer(11, "Top"), layer(12, "Layer 2")], 10);
    const r = resolve(cmd, play, new Map());
    expect(r.cmd.ids).toEqual([11, 12]);
    expect(r.cmd.name).toBe("keep");
    expect(r.notes.length).toBe(1);
    const lone = resolve(parameterize({ op: "layer.clear", id: 3 }, rec, new Map()), doc([layer(50, "Only")], 50), new Map());
    expect(lone.cmd.id).toBe(50);
    expect(lone.notes[0]).toMatch(/active layer/);
  });

  it("chooses the nearest recorded position among duplicate names", () => {
    const rec = doc([layer(1, "Layer"), layer(2, "Layer"), layer(3, "Layer")], 1);
    const cmd = parameterize({ op: "layer.props", id: 1, visible: false }, rec, new Map());
    const play = doc([layer(7, "Layer"), layer(8, "Layer"), layer(9, "Layer")], 9);
    expect(resolve(cmd, play, new Map()).cmd.id).toBe(7);
  });

  it("leaves ids it does not know and non-id params untouched, and filters non-replayable ops", () => {
    expect(parameterize({ op: "image.resize", width: 10, height: 10 }, null, new Map())).toEqual({ op: "image.resize", width: 10, height: 10 });
    expect(isReplayable("edit.undo")).toBe(false);
    expect(isReplayable("layer.set-active")).toBe(false);
    expect(isReplayable("analyze.histogram")).toBe(false);
    expect(isReplayable("filter.gaussian-blur")).toBe(true);
  });
});
