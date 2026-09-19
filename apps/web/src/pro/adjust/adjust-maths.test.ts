import { describe, expect, it } from "vitest";
import { DEFAULT_BANDS, bestRange, centered, exclude, hueOf, hueSatApply, include, invert, setHandle, weight, type Band, type HueSatParams } from "./hue-bands";
import { levelsFromSample, levelsMap, type LevelsValue } from "./levels-pick";

const ch = () => ({ in_black: 0, in_white: 255, gamma: 1, out_black: 0, out_white: 255 });
const levels = (): LevelsValue => ({ master: ch(), red: ch(), green: ch(), blue: ch() });

describe("hue bands", () => {
  const reds = DEFAULT_BANDS[0];
  it("weights are straight lines (same as adjust.rs)", () => {
    for (const [h, w] of [
      [0, 1],
      [345, 1],
      [15, 1],
      [330, 0.5],
      [30, 0.5],
      [315, 0],
      [45, 0],
      [180, 0],
    ])
      expect(weight(reds, h)).toBeCloseTo(w, 4);
    expect(weight(reds, 20)).toBeCloseTo(0.8333, 3);
  });
  it("centred on a hue keeps the shape", () => {
    const g = centered(DEFAULT_BANDS[2], 0);
    expect(weight(g, 0)).toBe(1);
    expect(weight(g, 120)).toBe(0);
    expect(g).toEqual([315, 345, 15, 45]);
  });
  it("include and exclude", () => {
    const inc = include(reds, 240);
    expect(weight(inc, 240)).toBe(1);
    expect(weight(inc, 0)).toBe(1);
    const exc = exclude(reds, 10);
    expect(weight(exc, 10)).toBe(0);
  });
  it("invert is the complement", () => {
    const inv = invert(reds);
    for (let h = 0; h < 360; h += 7) expect(weight(inv, h)).toBeCloseTo(1 - weight(reds, h), 6);
  });
  it("setHandle keeps the order", () => {
    expect(setHandle(reds, 1, 350)).toEqual([315, 350, 15, 45] as Band);
    expect(setHandle(reds, 1, 20)).toBeNull(); // range start past range end
  });
  it("picks the range that claims a hue", () => {
    expect(bestRange(DEFAULT_BANDS, 238)).toBe(4);
    expect(hueOf(128, 128, 128)).toBeNull();
    expect(hueOf(0, 0, 255)!.hue).toBe(240);
  });
  it("preview model agrees with the engine on simple cases", () => {
    const p: HueSatParams = {
      master: { hue: 0, saturation: 0, lightness: 0 },
      ranges: Array.from({ length: 6 }, () => ({ hue: 0, saturation: 0, lightness: 0 })),
      bands: DEFAULT_BANDS,
      colorize: false,
      colorize_hue: 0,
      colorize_saturation: 25,
      colorize_lightness: 0,
    };
    p.ranges[4].saturation = -100;
    expect(hueSatApply(p, [1, 0, 0])).toEqual([1, 0, 0]);
    hueSatApply(p, [0, 0, 1]).forEach((v) => expect(v).toBeCloseTo(0.5, 3));
  });
});

describe("levels droppers", () => {
  const sample = { r: 0.25 * 255, g: 0.4 * 255, b: 0.6 * 255 };
  it("black maps the sample to black", () => {
    const l = levelsFromSample(levels(), sample, "black");
    expect(levelsMap(l.red, sample.r)).toBeCloseTo(0, 0);
    expect(levelsMap(l.green, sample.g)).toBeCloseTo(0, 0);
    expect(levelsMap(l.blue, sample.b)).toBeCloseTo(0, 0);
  });
  it("white maps the sample to white", () => {
    const l = levelsFromSample(levels(), sample, "white");
    for (const k of ["red", "green", "blue"] as const) expect(levelsMap(l[k], sample[k[0] as "r" | "g" | "b"])).toBeGreaterThan(254);
  });
  it("gray neutralises the sample and keeps its luma", () => {
    const warm = { r: 150, g: 120, b: 90 };
    const l = levelsFromSample(levels(), warm, "gray");
    const out = [levelsMap(l.red, warm.r), levelsMap(l.green, warm.g), levelsMap(l.blue, warm.b)];
    expect(Math.abs(out[0] - out[1])).toBeLessThan(1.5);
    expect(Math.abs(out[1] - out[2])).toBeLessThan(1.5);
    const luma = 0.299 * warm.r + 0.587 * warm.g + 0.114 * warm.b;
    expect(Math.abs(out[1] - luma)).toBeLessThan(1.5);
    expect(l.master).toEqual(ch());
  });
});
