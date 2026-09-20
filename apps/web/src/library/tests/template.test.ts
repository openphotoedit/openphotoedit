import { describe, expect, it } from "vitest";
import { applyTemplate, sanitizeFileName, uniqueName } from "../template";

describe("filename templates", () => {
  const v = { name: "IMG_0042", n: 7, date: "2012-12-06T15:46:01", w: 2048, h: 1365, recipe: "Web", rating: 4, camera: "Canon EOS 5D Mark III", ext: "jpg" };
  it("fills every token", () => {
    expect(applyTemplate("{name}", v)).toBe("IMG_0042");
    expect(applyTemplate("{date}_{n:4}_{w}x{h}", v)).toBe("2012-12-06_0007_2048x1365");
    expect(applyTemplate("{recipe} - {name} ({rating}★) {time}", v)).toBe("Web - IMG_0042 (4★) 154601");
    expect(applyTemplate("{camera}/{name}", v)).toBe("Canon EOS 5D Mark III_IMG_0042");
    expect(applyTemplate("{n}", { name: "a", n: 12 })).toBe("12");
  });
  it("keeps unknown tokens visible and handles missing data", () => {
    expect(applyTemplate("{name}-{bogus}", v)).toBe("{name}-{bogus}".replace("{name}", "IMG_0042"));
    expect(applyTemplate("{date}", { name: "x", n: 1 })).toBe("undated");
    expect(applyTemplate("{w}x{h}", { name: "x", n: 1 })).toBe("{w}x{h}");
    expect(applyTemplate("{date}", { name: "x", n: 1, date: new Date(2024, 0, 5, 9, 8, 7) })).toBe("2024-01-05");
    expect(applyTemplate("", v)).toBe("IMG_0042");
  });
  it("produces safe names", () => {
    expect(sanitizeFileName('a/b\\c:d*e?f"g<h>i|j')).toBe("a_b_c_d_e_f_g_h_i_j");
    expect(sanitizeFileName("..hidden. ")).toBe("hidden");
    expect(sanitizeFileName("   ")).toBe("untitled");
    expect(sanitizeFileName("x".repeat(400)).length).toBe(180);
  });
  it("de-duplicates case-insensitively", () => {
    const taken = new Set<string>();
    expect(uniqueName("photo", "jpg", taken)).toBe("photo.jpg");
    expect(uniqueName("Photo", "jpg", taken)).toBe("Photo-2.jpg");
    expect(uniqueName("photo", "jpg", taken)).toBe("photo-3.jpg");
    expect(uniqueName("photo", "png", taken)).toBe("photo.png");
  });
});
