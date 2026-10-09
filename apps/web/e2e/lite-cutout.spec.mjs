// Lite's Select subject -> Remove background leaves the subject on transparency.
//
// The removal is limited to the selection when there is one, and in Lite the
// only selection at that moment is the subject itself, so the background
// stayed exactly where it was. This reads the exported PNG's alpha rather
// than trusting the absence of an error.
//
//   scripts/build-wasm.sh --dev && scripts/fetch-models.sh
//   cd apps/web && npx vite --port 5219 --strictPort
//   node apps/web/e2e/lite-cutout.spec.mjs
// Fails (exit 1) on any broken step.
import { chromium } from "playwright";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5219/";
const PHOTO = process.env.PHOTO ?? resolve(root, "testdata/photos/portrait.jpg");

const browser = await chromium.launch({ headless: !process.env.HEADED });
let failures = 0;

function check(ok, what) {
  if (ok) console.log(`  ok   ${what}`);
  else {
    failures++;
    console.log(`  FAIL ${what}`);
  }
}

const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, acceptDownloads: true });
// The File System Access picker is a native dialog; use the download path.
await context.addInitScript(() => {
  window.showSaveFilePicker = undefined;
});
const page = await context.newPage();
const idle = () => page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector(".lt-spin"), null, { timeout: 300_000 });

await page.goto(URL_);
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.getByTestId("choose-lite").click();
const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("open").click()]);
await chooser.setFiles(PHOTO);
await page.getByTestId("lite").waitFor({ timeout: 30_000 });
await idle();

await page.getByTestId("cat-retouch").first().click();
await page.getByTestId("select-subject").click();
await page.getByTestId("subject-actions").waitFor({ timeout: 300_000 });
await idle();
await page.getByTestId("subject-actions").getByRole("button", { name: "Remove background" }).click();
await page.waitForTimeout(500);
await idle();
const errs = (await page.locator(".toast.error").allTextContents()).map((s) => s.trim());
check(errs.length === 0, `Remove background shows no error${errs.length ? ` (${errs.join(" | ")})` : ""}`);

await page.getByTestId("export").click();
await page.getByTestId("format-png").click();
const [download] = await Promise.all([page.waitForEvent("download", { timeout: 120_000 }), page.getByTestId("export-save").click()]);
const png = readFileSync(await download.path()).toString("base64");
const alpha = await page.evaluate(async (b64) => {
  const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }), { premultiplyAlpha: "none" });
  const c = new OffscreenCanvas(bmp.width, bmp.height);
  const g = c.getContext("2d");
  g.drawImage(bmp, 0, 0);
  const at = (fx, fy) => g.getImageData(Math.floor(bmp.width * fx), Math.floor(bmp.height * fy), 1, 1).data[3];
  return { corner: at(0.03, 0.03), centre: at(0.5, 0.45) };
}, png);
check(alpha.corner === 0, `The background corner is transparent (alpha ${alpha.corner})`);
check(alpha.centre === 255, `The subject is still opaque (alpha ${alpha.centre})`);

await browser.close();
console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
