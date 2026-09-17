// Open a camera RAW through the real worker, wasm engine, raw-io opener and
// Camera Raw dialog: wait for the developed preview, raise exposure, open,
// and screenshot each step into target/raw/out/.
// Usage: node e2e/raw.mjs <file.cr3> [more files…]   (vite dev server on 5221)
import { chromium } from "@playwright/test";
import { readFileSync, mkdirSync } from "node:fs";
import { basename, resolve } from "node:path";

const url = process.env.URL ?? "http://localhost:5221/src/lib/raw-harness.html";
const outDir = resolve(import.meta.dirname, "../../../target/raw/out");
mkdirSync(outDir, { recursive: true });
const files = process.argv.slice(2);
const browser = await chromium.launch();
let failed = false;
for (const file of files) {
  const name = basename(file);
  const stem = name.replace(/\.[^.]+$/, "");
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const logs = [];
  page.on("console", (m) => logs.push(`${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => logs.push(`pageerror: ${e.message}`));
  await page.route("**/__raw-fixture", (route) => route.fulfill({ body: readFileSync(file), contentType: "application/octet-stream" }));
  await page.goto(url);
  await page.waitForFunction(() => window.rawHarnessReady === true);
  await page.evaluate(() => window.rawHarness.init());
  const t0 = Date.now();
  await page.evaluate(async (n) => {
    const bytes = new Uint8Array(await (await fetch("/__raw-fixture")).arrayBuffer());
    window.rawHarness.open(n, bytes);
  }, name);
  await page.getByTestId("raw-dialog").waitFor();
  const tDialog = Date.now() - t0;
  const timing = page.getByTestId("raw-timing");
  await page.waitForFunction(() => /First preview/.test(document.querySelector('[data-testid="raw-timing"]')?.textContent ?? ""), null, { timeout: 120_000 });
  const first = (await timing.textContent()).trim();
  await page.screenshot({ path: `${outDir}/web-${stem}-dialog.png` });

  // +1 EV: Shift+Arrow moves ten steps of 0.05.
  const exposure = page.getByTestId("raw-exposure").getByRole("slider");
  await exposure.focus();
  await exposure.press("Shift+ArrowRight");
  await exposure.press("Shift+ArrowRight");
  await page.waitForFunction(() => /last update/.test(document.querySelector('[data-testid="raw-timing"]')?.textContent ?? ""), null, { timeout: 60_000 });
  await page.waitForFunction(() => !document.querySelector('[data-testid="raw-dialog"] .spinner'), null, { timeout: 60_000 });
  const after = (await timing.textContent()).trim();
  const camera = (await page.getByTestId("raw-camera").textContent()).trim();
  await page.screenshot({ path: `${outDir}/web-${stem}-exposure.png` });

  const t1 = Date.now();
  await page.getByTestId("raw-open").click();
  const result = await page.evaluate(() => window.rawHarness.result);
  const tOpen = Date.now() - t1;
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${outDir}/web-${stem}-opened.png` });
  console.log(JSON.stringify({ name, camera, dialogMs: tDialog, first, after, openMs: tOpen, result }));
  if (!result.ok || (await page.getByTestId("raw-dialog").count()) !== 0) failed = true;
  const errors = logs.filter((l) => /^(error|pageerror)/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  await page.close();
}
await browser.close();
if (failed) process.exit(1);
