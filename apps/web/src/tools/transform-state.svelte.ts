// Live numbers of the free-transform box for the options bar's fields.
// Written by tools/transform.ts; read by TransformOptions.svelte.

export const transformFields = $state({
  /** A transform box exists (fields are editable). */
  ready: false,
  /** The box is a free-corner distort (fields other than X/Y are off). */
  distorted: false,
  /** Top-left of the unrotated box, document pixels. */
  x: 0,
  y: 0,
  w: 0,
  h: 0,
  /** Width as a percentage of the starting width. */
  scale: 100,
  /** Clockwise degrees. */
  angle: 0,
  /** How many layers the box transforms. */
  layers: 1,
});
