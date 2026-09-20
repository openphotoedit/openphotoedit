// Minimal XMP sidecars: rating, colour label and a pick/reject flag.
//
// Conventions, chosen to round-trip with the tools photographers already use:
// - `xmp:Rating` 0..5; a rejected photo is written as `xmp:Rating="-1"`,
//   which Lightroom, Bridge and darktable all read as "rejected".
// - `xmp:Label` is the colour name Lightroom and Bridge write ("Red" …).
// - Picks have no standard field, so they go in our own namespace as
//   `ops:Pick="1"` (and `ops:Pick="-1"` alongside the -1 rating for rejects).

import type { Marks } from "./types";

export const LABEL_NAMES = ["", "Red", "Yellow", "Green", "Blue", "Purple"] as const;
const NS_OPS = "https://openphotoedit.app/ns/library/1.0/";

function esc(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
}

export function sidecarName(fileName: string) {
  const i = fileName.lastIndexOf(".");
  return (i > 0 ? fileName.slice(0, i) : fileName) + ".xmp";
}

export function buildXmp(m: Marks): string {
  const rating = m.flag === -1 ? -1 : Math.max(0, Math.min(5, m.rating | 0));
  const attrs = [`xmp:Rating="${rating}"`];
  if (m.label > 0 && m.label < LABEL_NAMES.length) attrs.push(`xmp:Label="${esc(LABEL_NAMES[m.label])}"`);
  if (m.flag !== 0) attrs.push(`ops:Pick="${m.flag}"`);
  if (m.flag === -1 && m.rating > 0) attrs.push(`ops:Stars="${m.rating}"`);
  return `<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="OpenPhotoEdit">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:ops="${NS_OPS}"
    ${attrs.join("\n    ")}/>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
`;
}

function attrOrElement(xml: string, name: string): string | undefined {
  const a = new RegExp(`${name}\\s*=\\s*"([^"]*)"`).exec(xml);
  if (a) return a[1];
  const e = new RegExp(`<${name}>([^<]*)</${name}>`).exec(xml);
  return e?.[1];
}

/** Read marks from a sidecar written by us, Lightroom, Bridge, Capture One or darktable. */
export function parseXmp(xml: string): Marks | null {
  if (!/xmpmeta|rdf:RDF/.test(xml)) return null;
  const ratingS = attrOrElement(xml, "xmp:Rating");
  const labelS = attrOrElement(xml, "xmp:Label");
  const pickS = attrOrElement(xml, "ops:Pick");
  const starsS = attrOrElement(xml, "ops:Stars");
  if (ratingS == null && labelS == null && pickS == null) return null;
  const rating = ratingS != null ? Math.round(Number(ratingS)) : 0;
  const pick = pickS != null ? Math.sign(Math.round(Number(pickS))) : 0;
  const flag = (rating < 0 ? -1 : pick) as -1 | 0 | 1;
  const stars = rating < 0 ? Number(starsS ?? 0) : rating;
  const labelIdx = labelS ? LABEL_NAMES.findIndex((n) => n && n.toLowerCase() === labelS.trim().toLowerCase()) : 0;
  return {
    rating: Number.isFinite(stars) ? Math.max(0, Math.min(5, stars)) : 0,
    flag,
    label: Math.max(0, labelIdx),
  };
}
