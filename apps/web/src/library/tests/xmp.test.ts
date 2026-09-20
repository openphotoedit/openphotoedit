import { describe, expect, it } from "vitest";
import { buildXmp, parseXmp, sidecarName } from "../xmp";

describe("XMP sidecars", () => {
  it("round-trips every mark", () => {
    for (const rating of [0, 3, 5])
      for (const flag of [-1, 0, 1] as const)
        for (const label of [0, 1, 4, 5]) {
          const m = { rating, flag, label };
          expect(parseXmp(buildXmp(m))).toEqual(m);
        }
  });
  it("writes the conventions other tools read", () => {
    const x = buildXmp({ rating: 2, flag: -1, label: 1 });
    expect(x).toContain('xmp:Rating="-1"');
    expect(x).toContain('xmp:Label="Red"');
    expect(buildXmp({ rating: 4, flag: 0, label: 0 })).toContain('xmp:Rating="4"');
  });
  it("reads a Lightroom-style sidecar with element syntax", () => {
    const lr = `<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/"><xmp:Rating>3</xmp:Rating><xmp:Label>Green</xmp:Label></rdf:Description></rdf:RDF></x:xmpmeta>`;
    expect(parseXmp(lr)).toEqual({ rating: 3, flag: 0, label: 3 });
    expect(parseXmp("not xml")).toBeNull();
  });
  it("names sidecars like Lightroom", () => {
    expect(sidecarName("IMG_0001.CR2")).toBe("IMG_0001.xmp");
    expect(sidecarName("noext")).toBe("noext.xmp");
  });
});
