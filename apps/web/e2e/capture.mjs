// Screenshots for the test guide that the feature specs do not already take:
// the first-run chooser, Lite in dark mode, a real multi-layer PSD, a camera
// RAW opened through the app, and a 24 MP document. Against the production
// build: npx vite preview --port 5203 --strictPort
//   node e2e/capture.mjs <outDir>
import { chromium } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const OUT = resolve(process.argv[2] ?? resolve(root, "target/ship/shots"));
const URL_ = process.env.URL ?? "http://localhost:5203/";
mkdirSync(OUT, { recursive: true });
const facts = {};

const browser = await chromium.launch();

async function fresh(opts) {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, ...opts });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => console.log("pageerror", e.message));
  await page.goto(URL_);
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  return { ctx, page };
}
const idle = (page) => page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'), null, { timeout: 120_000 });
async function openVia(page, file, button) {
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), button.click()]);
  await chooser.setFiles(file);
}

// 01 — first run, light
{
  const { ctx, page } = await fresh({ colorScheme: "light" });
  await page.getByTestId("choose-lite").waitFor();
  await page.screenshot({ path: `${OUT}/01-chooser.png` });
  await ctx.close();
}

// 02 — Lite in dark mode with a landscape
{
  const { ctx, page } = await fresh({ colorScheme: "dark" });
  await page.getByTestId("choose-lite").click();
  await page.getByTestId("lite-start").waitFor();
  await openVia(page, resolve(root, "testdata/photos/landscape.jpg"), page.getByTestId("open"));
  await page.getByTestId("lite").waitFor({ timeout: 30_000 });
  await idle(page);
  await page.getByTestId("cat-effects").first().click();
  await page.locator('[data-testid="look-film"] img').waitFor({ timeout: 60_000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}/02-lite-dark-looks.png` });
  await ctx.close();
}

// 03 — Pro opening a real multi-layer PSD
{
  const { ctx, page } = await fresh({ colorScheme: "dark" });
  await page.getByTestId("choose-pro").click();
  const psd = resolve(root, "testdata/psd/psd-tools/layer_effects.psd");
  await openVia(page, psd, page.getByRole("button", { name: /^open/i }).first());
  await page.waitForFunction(() => document.querySelectorAll('[data-testid="layer-row"], .layer-row, [role="treeitem"]').length > 3, null, { timeout: 60_000 }).catch(() => {});
  await idle(page);
  await page.waitForTimeout(1200);
  facts.psdLayers = await page.evaluate(() => document.querySelectorAll('[data-testid="layer-row"], .layer-row, [role="treeitem"]').length);
  await page.screenshot({ path: `${OUT}/03-pro-psd.png` });
  await ctx.close();
}

// 04 — Camera RAW through the app's own opener
{
  const { ctx, page } = await fresh({ colorScheme: "dark" });
  await page.getByTestId("choose-pro").click();
  await openVia(page, resolve(root, "testdata/raw/nikon-z6.nef"), page.getByRole("button", { name: /^open/i }).first());
  await page.getByTestId("raw-dialog").waitFor({ timeout: 60_000 });
  await page.waitForFunction(() => /First preview/.test(document.querySelector('[data-testid="raw-timing"]')?.textContent ?? ""), null, { timeout: 120_000 });
  facts.rawPreview = (await page.getByTestId("raw-timing").textContent())?.trim();
  await page.screenshot({ path: `${OUT}/04-raw-dialog.png` });
  await ctx.close();
}

// 05 — a 24 MP document with adjustment layers, generated in the page
{
  const { ctx, page } = await fresh({ colorScheme: "dark" });
  await page.getByTestId("choose-pro").click();
  await page.waitForTimeout(800);
  const png = await page.evaluate(async () => {
    const w = 6000, h = 4000;
    const c = new OffscreenCanvas(w, h);
    const g = c.getContext("2d");
    for (let i = 0; i < 60; i++) {
      const gr = g.createLinearGradient(0, (i * h) / 60, w, ((i + 1) * h) / 60);
      gr.addColorStop(0, `hsl(${i * 6} 55% 45%)`);
      gr.addColorStop(1, `hsl(${(i * 6 + 140) % 360} 60% 60%)`);
      g.fillStyle = gr;
      g.fillRect(0, (i * h) / 60, w, h / 60 + 1);
    }
    for (let i = 0; i < 3000; i++) {
      g.fillStyle = `hsla(${(i * 47) % 360} 70% 50% / 0.7)`;
      g.beginPath();
      g.arc((i * 7919) % w, (i * 104729) % h, 20 + (i % 90), 0, Math.PI * 2);
      g.fill();
    }
    const b = await c.convertToBlob({ type: "image/png" });
    return Array.from(new Uint8Array(await b.arrayBuffer()));
  });
  const file = resolve(OUT, "../large-24mp.png");
  writeFileSync(file, Buffer.from(png));
  const t0 = Date.now();
  await openVia(page, file, page.getByRole("button", { name: /^open/i }).first());
  await page.waitForFunction(() => /6000\s*×\s*4000/.test(document.body.innerText), null, { timeout: 120_000 });
  await idle(page);
  facts.open24mpMs = Date.now() - t0;
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/05-pro-24mp.png` });
  await ctx.close();
}

writeFileSync(`${OUT}/facts.json`, JSON.stringify(facts, null, 1));
console.log(JSON.stringify(facts));
await browser.close();
