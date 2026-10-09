// Times filters in the browser, on the same 24 MP photo the native
// benchmark uses, through the same wasm the app ships.
//
//   scripts/build-wasm.sh                       # release wasm + glue
//   cd apps/web && npx vite --port 5272 --strictPort &
//   node apps/web/e2e/filter-bench.mjs
//
// `URL` overrides the origin, `MP` the megapixels, `REPS` the repeat count
// (the best of N is reported), `ONLY` a comma-separated list of substrings
// of the op name.
import { readFileSync } from "node:fs";
import { chromium } from "playwright";

const origin = process.env.URL ?? "http://localhost:5272";
const mp = Number(process.env.MP ?? 24);
const reps = Number(process.env.REPS ?? 1);
const only = process.env.ONLY ?? "";

const h = Math.round(Math.sqrt((mp * 1e6) / 1.5) / 2) * 2;
const w = Math.floor((h * 3) / 2 / 2) * 2;

const cases = [
  { op: "filter.surface-blur", radius: 10, threshold: 15 },
  { op: "filter.surface-blur", radius: 12, threshold: 20 },
  { op: "filter.surface-blur", radius: 40, threshold: 50 },
  { op: "filter.radial-blur", amount: 20 },
  { op: "filter.clouds" },
  { op: "filter.reduce-noise" },
  { op: "filter.add-noise", amount: 10, gaussian: true },
  { op: "filter.add-noise", amount: 10 },
  { op: "filter.gaussian-blur", radius: 20 },
  { op: "filter.box-blur", radius: 20 },
  { op: "filter.motion-blur", angle: 30, distance: 40 },
  { op: "filter.tilt-shift", center_y: 1000, band: 600, feather: 400, radius: 20 },
  { op: "filter.median", radius: 5 },
  { op: "filter.unsharp-mask", amount: 100, radius: 3, threshold: 2 },
];

const jpeg = readFileSync(new URL("../../../testdata/photos/landscape.jpg", import.meta.url));

const browser = await chromium.launch();
const page = await browser.newPage();
page.on("console", (m) => {
  if (m.type() === "error") console.error("page:", m.text());
});
page.on("pageerror", (e) => console.error("pageerror:", e.message));

await page.goto(`${origin}/e2e/filter-bench.html`);
await page.waitForFunction(() => typeof window.benchSetup === "function");
await page.evaluate(([bytes, w, h]) => window.benchSetup(bytes, w, h), [[...jpeg], w, h]);
console.log(`${w} × ${h} (${((w * h) / 1e6).toFixed(1)} MP), best of ${reps}`);

const rows = [];
for (const cmd of cases) {
  const name = Object.entries(cmd)
    .filter(([k]) => k !== "op")
    .map(([k, v]) => `${k[0]}${v}`)
    .join(" ");
  const label = `${cmd.op} ${name}`.trim();
  if (only && !only.split(",").some((o) => label.includes(o.trim()))) continue;
  const ms = await page.evaluate(([cmd, reps]) => window.benchRun(cmd, reps), [cmd, reps]);
  rows.push([label, ms]);
  console.log(`${label.padEnd(38)} ${ms.toFixed(0).padStart(8)} ms ${(ms / ((w * h) / 1e6)).toFixed(1).padStart(8)} ms/MP`);
}

console.log("\nslowest first:");
for (const [label, ms] of [...rows].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${label.padEnd(38)} ${ms.toFixed(0).padStart(8)} ms`);
}
await browser.close();
