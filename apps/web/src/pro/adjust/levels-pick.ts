// Levels black, gray and white point eyedroppers: turn one sampled colour
// into per-channel Levels settings, the way Photoshop's droppers do.
//
// Levels maps a channel value v (0..255) as
//   t = clamp((v − in_black) / (in_white − in_black), 0, 1)
//   out = out_black + (out_white − out_black) · t^(1/gamma)
// (crates/editor-core/src/adjust.rs, `LevelsChannel::lut`), per channel and
// then through the master (RGB) channel.
//
// Black point: each channel's in_black becomes the sample's value there, so
//   the sample maps to that channel's output black.
// White point: each channel's in_white becomes the sample's value.
// Gray point: each channel keeps its black and white points and gets the
//   gamma that sends the sample's normalised value t_c to a common target T:
//   t_c^(1/gamma_c) = T  ⇒  gamma_c = ln t_c / ln T.
//   T is the sample's own Rec.601 luma in the normalised domain,
//   T = 0.299·t_r + 0.587·t_g + 0.114·t_b, so the cast goes and the
//   brightness stays (Photoshop's midtone dropper ignores the target
//   colour's brightness for the same reason). Gammas clamp to 0.01..9.99,
//   the range Levels accepts.
// The master channel is left as it is: it applies equally to R, G and B, so
// it cannot bring back a cast.

export interface LevelsChannel {
  in_black: number;
  in_white: number;
  gamma: number;
  out_black: number;
  out_white: number;
}
export interface LevelsValue {
  master: LevelsChannel;
  red: LevelsChannel;
  green: LevelsChannel;
  blue: LevelsChannel;
}
export type Dropper = "black" | "gray" | "white";

const CHANNELS = ["red", "green", "blue"] as const;

export function levelsFromSample(value: LevelsValue, sample: { r: number; g: number; b: number }, dropper: Dropper): LevelsValue {
  const v = [sample.r, sample.g, sample.b];
  const next = structuredClone(value);
  if (dropper === "black" || dropper === "white") {
    CHANNELS.forEach((k, i) => {
      const ch = next[k];
      if (dropper === "black") ch.in_black = Math.max(0, Math.min(Math.round(v[i]), ch.in_white - 2));
      else ch.in_white = Math.min(255, Math.max(Math.round(v[i]), ch.in_black + 2));
    });
    return next;
  }
  const t = CHANNELS.map((k, i) => {
    const ch = next[k];
    return (v[i] - ch.in_black) / Math.max(1, ch.in_white - ch.in_black);
  });
  const target = 0.299 * t[0] + 0.587 * t[1] + 0.114 * t[2];
  if (!(target > 0 && target < 1)) return next;
  CHANNELS.forEach((k, i) => {
    if (t[i] > 0 && t[i] < 1) next[k].gamma = Math.round(Math.min(9.99, Math.max(0.01, Math.log(t[i]) / Math.log(target))) * 100) / 100;
  });
  return next;
}

/** A channel value through one Levels channel (for tests and readouts). */
export function levelsMap(ch: LevelsChannel, v: number): number {
  const t = Math.min(1, Math.max(0, (v - ch.in_black) / Math.max(1e-6, ch.in_white - ch.in_black)));
  return ch.out_black + (ch.out_white - ch.out_black) * Math.pow(t, 1 / ch.gamma);
}
