/// <reference types="node" />
import { readFileSync, readdirSync, statSync, openSync, readSync, closeSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { captureTime, embeddedJpegCandidates, formatAperture, formatExposure, formatFocal, isDecodableJpeg, parseExif, psdThumbnail, sniff } from "../exif";

const ROOT = resolve(__dirname, "../../../../../testdata");
const photo = (n: string) => new Uint8Array(readFileSync(join(ROOT, "photos", n)));

function head(path: string, n: number) {
  const fd = openSync(path, "r");
  const buf = Buffer.alloc(n);
  const got = readSync(fd, buf, 0, n, 0);
  closeSync(fd);
  return new Uint8Array(buf.buffer, buf.byteOffset, got);
}

function range(path: string, offset: number, n: number) {
  const fd = openSync(path, "r");
  const buf = Buffer.alloc(n);
  const got = readSync(fd, buf, 0, n, offset);
  closeSync(fd);
  return new Uint8Array(buf.buffer, buf.byteOffset, got);
}

describe("EXIF from real JPEGs", () => {
  it("reads the portrait's camera, lens and exposure", () => {
    const e = parseExif(photo("portrait.jpg"));
    expect(e.make).toBe("Canon");
    expect(e.model).toBe("Canon EOS 5D Mark III");
    expect(e.lens).toBe("EF85mm f/1.2L USM");
    expect(e.iso).toBe(200);
    expect(e.exposureTime).toBeCloseTo(0.008, 6);
    expect(e.fNumber).toBeCloseTo(7.1, 6);
    expect(e.focalLength).toBe(85);
    expect(e.dateTaken).toBe("2012-12-06T15:46:01");
    expect(e.orientation).toBe(1);
    expect([e.width, e.height]).toEqual([2687, 3356]);
    expect(formatExposure(e.exposureTime)).toBe("1/125 s");
    expect(formatAperture(e.fNumber)).toBe("f/7.1");
    expect(formatFocal(e.focalLength)).toBe("85 mm");
  });

  it("reads only the head of the file (64 KB is enough for APP1)", () => {
    const e = parseExif(photo("portrait.jpg").subarray(0, 65536));
    expect(e.model).toBe("Canon EOS 5D Mark III");
    expect(e.iso).toBe(200);
  });

  it("matches PIL on every photo in testdata", () => {
    const land = parseExif(photo("landscape.jpg"));
    expect([land.make, land.model, land.iso, land.fNumber, land.exposureTime, land.focalLength]).toEqual(["SONY", "DSC-H2", 80, 5, 0.001, 13.1]);
    expect(land.dateTaken).toBe("2014-08-06T00:00:00");
    expect([land.width, land.height]).toEqual([2400, 1800]);
    const prod = parseExif(photo("product.jpg"));
    expect([prod.make, prod.model, prod.iso, prod.fNumber]).toEqual(["SONY", "DSC-RX100", 800, 3.2]);
    expect(formatExposure(prod.exposureTime)).toBe("1/30 s");
    expect(prod.focalLength).toBeCloseTo(15.64, 2);
    expect(prod.dateTaken).toBe("2013-07-04T11:47:48");
    const bw = parseExif(photo("old-bw.jpg"));
    expect(bw.make).toBeUndefined();
    expect(bw.dateTaken).toBe("2005-05-25T10:21:51"); // DateTimeDigitized before the edit date
    expect([bw.width, bw.height]).toEqual([1231, 1600]);
    const street = parseExif(photo("street.jpg"));
    expect(street.model).toBeUndefined();
    expect([street.width, street.height]).toEqual([2400, 1600]);
  });

  it("never throws on truncated or corrupt input", () => {
    const p = photo("portrait.jpg");
    for (let n = 0; n < 2000; n += 7) expect(() => parseExif(p.subarray(0, n))).not.toThrow();
    const bad = p.slice(0, 70000);
    for (let i = 20; i < 2000; i += 13) bad[i] ^= 0xa5;
    expect(() => parseExif(bad)).not.toThrow();
    expect(parseExif(new Uint8Array([0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff]))).toBeTypeOf("object");
    expect(parseExif(new Uint8Array(0))).toEqual({});
  });

  it("computes a sortable capture time", () => {
    expect(captureTime({ dateTaken: "2012-12-06T15:46:01" })).toBe(Date.UTC(2012, 11, 6, 15, 46, 1));
    expect(captureTime({})).toBeUndefined();
  });
});

describe("raw files", () => {
  const dir = join(ROOT, "raw");
  const files = readdirSync(dir).filter((f) => !f.startsWith(".") && !f.endsWith(".part"));

  it("reads camera make and model from every TIFF-based raw and CR3", () => {
    const seen: Record<string, string | undefined> = {};
    for (const f of files) {
      const e = parseExif(head(join(dir, f), 1 << 20), { ext: f.split(".").pop() });
      seen[f] = e.model;
      if (!f.endsWith(".raf")) expect(e.make, f).toBeTruthy();
      expect(e.model, f).toBeTruthy();
      expect(e.dateTaken, f).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    }
    expect(seen["canon-eos-7d-mark-ii.cr2"]).toContain("7D Mark II");
    expect(seen["nikon-z6.nef"]).toMatch(/Z 6/);
    expect(seen["canon-eos-r8.cr3"]).toMatch(/R8/);
  });

  it("finds a decodable embedded JPEG in CR2, NEF, ARW, RAF", () => {
    for (const f of ["canon-eos-7d-mark-ii.cr2", "nikon-z6.nef", "sony-a7c.arw", "fujifilm-x-s10.raf"]) {
      const p = join(dir, f);
      const size = statSync(p).size;
      const c = embeddedJpegCandidates(head(p, 1 << 20), size);
      const ok = c.find((r) => isDecodableJpeg(range(p, r.offset, 65536)));
      expect(ok, f).toBeTruthy();
      expect(ok!.length, f).toBeGreaterThan(50_000);
    }
  });

  it("sniffs formats", () => {
    expect(sniff(photo("portrait.jpg"))).toBe("jpeg");
    expect(sniff(head(join(dir, "canon-eos-r8.cr3"), 64))).toBe("cr3");
    expect(sniff(head(join(dir, "fujifilm-x-s10.raf"), 64))).toBe("raf");
  });
});

describe("PSD thumbnail", () => {
  it("finds the JPEG thumbnail resource in a real PSD", () => {
    const psds: string[] = [];
    const walk = (d: string) => {
      for (const f of readdirSync(d)) {
        const p = join(d, f);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.psd$/i.test(f)) psds.push(p);
      }
    };
    walk(join(ROOT, "psd"));
    let found = 0;
    for (const p of psds) {
      const r = psdThumbnail(head(p, 1 << 20));
      if (r && isDecodableJpeg(range(p, r.offset, 4096))) found++;
    }
    expect(psds.length).toBeGreaterThan(0);
    expect(found).toBeGreaterThan(0);
  });
});
