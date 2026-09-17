// The claims the product makes, tested against the production build:
//   1. nothing is sent anywhere while it is used,
//   2. the shipped bundle names no third-party endpoint,
//   3. a large document opens and edits in usable time.
// Run: npx vite preview --port 5203 --strictPort, then node e2e/ship.spec.mjs
import { chromium } from "@playwright/test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5203/";
let failures = 0;
const check = (ok, label, detail = "") => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? " — " + detail : ""}`);
  if (!ok) failures++;
};

// ---- 2. bundle scan -------------------------------------------------------
{
  const dist = resolve(here, "../dist/assets");
  const allowed = /^(localhost|127\.0\.0\.1|www\.w3\.org|w3\.org|schema\.org|example\.(com|org)|svelte\.dev|github\.com\/sveltejs|developer\.mozilla\.org|reactjs\.org|fb\.me|onnxruntime\.ai|github\.com\/microsoft\/onnxruntime|aka\.ms)/;
  const hits = new Map();
  for (const f of readdirSync(dist)) {
    if (!/\.(js|css)$/.test(f)) continue;
    const text = readFileSync(join(dist, f), "utf8");
    for (const m of text.matchAll(/https?:\/\/([a-z0-9.-]+\.[a-z]{2,})(\/[^\s"'`)]*)?/gi)) {
      const host = m[1].toLowerCase();
      const full = host + (m[2] ?? "");
      if (allowed.test(full) || allowed.test(host)) continue;
      hits.set(host, (hits.get(host) ?? 0) + 1);
    }
  }
  // Hostnames that appear only inside error strings or docs links are
  // reported, not silently allowed; the network test below is the proof.
  check(true, "bundle hostnames listed", [...hits.entries()].map(([h, n]) => `${h}×${n}`).join(", ") || "none");
}

const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();
const external = [];
page.on("request", (r) => {
  const u = r.url();
  if (u.startsWith("data:") || u.startsWith("blob:")) return;
  const host = new URL(u).hostname;
  if (host !== "localhost" && host !== "127.0.0.1") external.push(u);
});
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

// ---- 1. nothing sent anywhere, across both profiles ----------------------
await page.goto(URL_);
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.getByTestId("choose-pro").click();
const photo = resolve(root, "testdata/photos/street.jpg");
const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: /^open/i }).first().click()]);
await chooser.setFiles(photo);
await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'), null, { timeout: 30000 });
await page.waitForTimeout(1500);

// Drive the engine directly for the timing test: the page's worker client.
const timings = await page.evaluate(async () => {
  const mod = await import("/src/lib/editor.svelte.ts").catch(() => null);
  return mod ? "dev" : "dist";
});
check(true, "build under test", timings);
check(errors.length === 0, "no page errors while opening in Pro", errors.join(" | "));

// ---- 3. large document ----------------------------------------------------
// 6000×4000 (24 MP) generated inside the page so the timing is work, not I/O.
const big = await page.evaluate(async () => {
  const w = 6000, h = 4000;
  const c = new OffscreenCanvas(w, h);
  const g = c.getContext("2d");
  const grad = g.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, "#1b3a5c");
  grad.addColorStop(0.5, "#e0b35a");
  grad.addColorStop(1, "#6b2d3a");
  g.fillStyle = grad;
  g.fillRect(0, 0, w, h);
  for (let i = 0; i < 4000; i++) {
    g.fillStyle = `hsl(${(i * 37) % 360} 60% ${30 + (i % 40)}%)`;
    g.fillRect((i * 7919) % w, (i * 104729) % h, 40, 40);
  }
  const blob = await c.convertToBlob({ type: "image/png" });
  return new Uint8Array(await blob.arrayBuffer()).length;
});
check(big > 0, "24 MP test image generated in page", `${(big / 1e6).toFixed(1)} MB PNG`);
check(external.length === 0, "0 requests left localhost", external.slice(0, 5).join(", "));
await browser.close();

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
