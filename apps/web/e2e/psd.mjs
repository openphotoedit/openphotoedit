// Open a PSD through the real worker, wasm engine and psd-io openers; export
// it, save it as a project, reopen both, and screenshot the result.
// Usage: node e2e/psd.mjs <file.psd> <out.png>   (vite dev server on 5214)
import { chromium } from "playwright";
import { readFileSync } from "node:fs";
import { basename } from "node:path";

const url = process.env.URL ?? "http://localhost:5214/e2e/psd-harness.html";
const file = process.argv[2];
const out = process.argv[3] ?? "psd.png";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const logs = [];
page.on("console", (m) => logs.push(`${m.type()}: ${m.text()}`));
page.on("pageerror", (e) => logs.push(`pageerror: ${e.message}`));
await page.goto(url);
await page.waitForFunction(() => window.psdHarnessReady === true);
const b64 = readFileSync(file).toString("base64");
const result = await page.evaluate(([name, data]) => window.psdHarness.run(name, data), [basename(file), b64]);
await page.waitForTimeout(300);
await page.screenshot({ path: out, fullPage: true });
console.log(JSON.stringify(result, null, 2));
if (logs.length) console.log(logs.join("\n"));
await browser.close();
if (!result.sameTree) process.exit(1);
