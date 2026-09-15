// Which part of the active layer painting tools write to: its pixels or its
// mask. Set by clicking a layer's thumbnail or mask thumbnail in the Layers
// panel (Photoshop's "target"). Tools read `paintTarget.value` and pass it
// as `target` to `paint.stroke` / `paint.gradient`.
//
// `quickMask` is Photoshop's Quick Mask mode: while on, the Pro shell tints
// unselected areas and painting is meant to edit the selection.

export type PaintTarget = "pixels" | "mask";

export const paintTarget = $state({
  value: "pixels" as PaintTarget,
  /** The layer the mask target belongs to; the target resets when it changes. */
  layerId: null as number | null,
  quickMask: false,
});

export function setPaintTarget(value: PaintTarget, layerId: number | null) {
  paintTarget.value = value;
  paintTarget.layerId = layerId;
}
