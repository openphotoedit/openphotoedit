// Transient tool state the option components show (not persisted): whether
// a crop or transform is waiting for Enter, sub-modes, sampled colours.

import type { Rgba8 } from "../engine/types";

export const toolState = $state({
  /** A crop box differs from the whole canvas or is being edited. */
  cropPending: false,
  /** Crop tool: the next drag draws a horizon line instead of a box. */
  cropStraighten: false,
  /** Crop box size and angle, for the options readout. */
  cropInfo: { w: 0, h: 0, angle: 0 },
  perspectivePending: false,
  transformActive: false,
  /** Transform readout. */
  transformInfo: { w: 0, h: 0, angle: 0 },
  textEditing: false,
  /** Clone/heal source is set. */
  sourceSet: false,
  /** Last colour the eyedropper sampled. */
  sampled: null as Rgba8 | null,
  /** A shape or text layer is selected for editing by an annotation tool. */
  annotationSelected: null as number | null,
});
