/// <reference types="node" />
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { BLUR_ABS, dHash, exposureBadge, groupDuplicates, hamming, judge, measure, pHash, sharpness, suggestRejects, type CullInput } from "../cull";

const ROOT = resolve(__dirname, "../../../../../testdata");
const OUT = resolve(__dirname, "../../../../../target/library/proxies");
const PHOTOS = ["portrait", "street", "landscape", "product", "old-bw"];
let index: Record<string, [number, number]> = {};

function gray(name: string) {
  const [w, h] = index[name];
  const lum = new Uint8Array(readFileSync(join(OUT, name + ".gray")));
  const rgba = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    rgba[i * 4] = rgba[i * 4 + 1] = rgba[i * 4 + 2] = lum[i];
    rgba[i * 4 + 3] = 255;
  }
  return { lum, w, h, rgba };
}

beforeAll(() => {
  if (!existsSync(join(OUT, "index.json")) || process.env.REGEN) execFileSync("python3", [join(__dirname, "proxies.py"), ROOT, OUT], { stdio: "inherit" });
  index = JSON.parse(readFileSync(join(OUT, "index.json"), "utf8"));
}, 120_000);

describe("perceptual hashes", () => {
  it("are stable under recompression, rescaling and a slight crop", () => {
    for (const p of PHOTOS) {
      const a = gray(p);
      const hd = dHash(a.lum, a.w, a.h);
      const hp = pHash(a.lum, a.w, a.h);
      expect(hd).toMatch(/^[0-9a-f]{16}$/);
      expect(dHash(a.lum, a.w, a.h)).toBe(hd); // deterministic
      for (const v of ["-recompressed", "-half"]) {
        const b = gray(p + v);
        expect(hamming(hd, dHash(b.lum, b.w, b.h)), p + v).toBeLessThanOrEqual(3);
        expect(hamming(hp, pHash(b.lum, b.w, b.h)), p + v).toBeLessThanOrEqual(4);
      }
      const c = gray(p + "-crop");
      expect(hamming(hd, dHash(c.lum, c.w, c.h)), p + "-crop").toBeLessThanOrEqual(12);
    }
  });

  it("separate different photos", () => {
    for (let i = 0; i < PHOTOS.length; i++)
      for (let j = i + 1; j < PHOTOS.length; j++) {
        const a = gray(PHOTOS[i]);
        const b = gray(PHOTOS[j]);
        const d = hamming(dHash(a.lum, a.w, a.h), dHash(b.lum, b.w, b.h));
        const pd = hamming(pHash(a.lum, a.w, a.h), pHash(b.lum, b.w, b.h));
        expect(d > 12 || pd > 14, `${PHOTOS[i]} vs ${PHOTOS[j]}: d=${d} p=${pd}`).toBe(true);
      }
  });
});

describe("quality signals", () => {
  it("finds blur: every blurred variant is far softer than its original", () => {
    for (const p of PHOTOS) {
      const a = gray(p);
      const b = gray(p + "-blur");
      const sa = sharpness(a.lum, a.w, a.h);
      const sb = sharpness(b.lum, b.w, b.h);
      expect(sb, p).toBeLessThan(sa * 0.3);
      expect(sb, p).toBeLessThan(BLUR_ABS);
    }
  });

  it("flags over- and under-exposure but not the originals", () => {
    for (const p of PHOTOS) {
      expect(exposureBadge(measure(gray(p).rgba, index[p][0], index[p][1])), p).toBeNull();
      expect(exposureBadge(measure(gray(p + "-over").rgba, ...index[p + "-over"])), p).toBe("over");
      expect(exposureBadge(measure(gray(p + "-under").rgba, ...index[p + "-under"])), p).toBe("under");
    }
  });

  it("groups copies, picks the sharp frame as best and suggests only the bad frames", () => {
    const names = PHOTOS.flatMap((p) => [p, p + "-recompressed", p + "-blur", p + "-over"]);
    const items: CullInput[] = names.map((n) => {
      const g = gray(n);
      return { path: n, ...measure(g.rgba, g.w, g.h), faces: null, eyesClosed: null };
    });
    const groups = groupDuplicates(items);
    for (const p of PHOTOS) {
      const gi = groups[names.indexOf(p)];
      expect(gi, p).toBeGreaterThanOrEqual(0);
      expect(groups[names.indexOf(p + "-recompressed")], p).toBe(gi);
      expect(groups[names.indexOf(p + "-blur")], p).toBe(gi);
    }
    const res = judge(items);
    for (const p of PHOTOS) {
      expect(res.get(p + "-blur")!.badges, p).toContain("blurry");
      expect(res.get(p)!.badges, p).not.toContain("blurry");
      expect(res.get(p + "-over")!.badges, p).toContain("over");
      const best = [p, p + "-recompressed"].some((n) => res.get(n)!.badges.includes("best"));
      expect(best, p).toBe(true);
    }
    const rejects = new Set(suggestRejects(res));
    for (const p of PHOTOS) {
      expect(rejects.has(p + "-blur")).toBe(true);
      expect(rejects.has(p)).toBe(false);
    }
  });

  it("does not group frames far apart in time unless near-identical", () => {
    const a = { dhash: "ffff0000ffff0000", phash: "0f0f0f0f0f0f0f0f", capture: 0 };
    const b = { dhash: "ffff0000ffff00ff", phash: "0f0f0f0f0f0f0fff", capture: 3_600_000 }; // 8 bits each apart
    expect(groupDuplicates([a, b])).toEqual([-1, -1]);
    expect(groupDuplicates([a, { ...b, capture: 1000 }])).toEqual([0, 0]);
  });
});
