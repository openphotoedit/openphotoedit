// Lite's Enhance runs on the photo whatever Lite added last.
//
// Lite shows no layer list, and its sliders, Looks and captions each add a
// layer that becomes the active one. The Enhance models write into a pixel
// layer, so after an Adjust they used to stop at "Select a pixel layer to
// upscale." with no way to pick one short of opening Pro.
//
//   scripts/build-wasm.sh --dev && scripts/fetch-models.sh
//   cd apps/web && npx vite --port 5219 --strictPort
//   node apps/web/e2e/lite-enhance.spec.mjs
// Fails (exit 1) on any broken step.
import { chromium } from "playwright";
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

const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const idle = () => page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'), null, { timeout: 60_000 });
const errors = () => page.locator(".toast.error").allTextContents();
const dims = async () => {
  await page.getByTestId("cat-crop").first().click();
  await page.getByTestId("crop-dims").waitFor();
  const [w, h] = (await page.getByTestId("crop-dims").textContent()).split("×").map((s) => Number(s.trim()));
  return { w, h };
};

await page.goto(URL_);
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.getByTestId("choose-lite").click();
const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("open").click()]);
await chooser.setFiles(PHOTO);
await page.getByTestId("lite").waitFor({ timeout: 30_000 });
await idle();

// The first slider move adds Lite's adjustment layer, and it becomes active.
await page.getByTestId("cat-adjust").first().click();
await page.getByTestId("slider-light").fill("30");
await page.waitForTimeout(800);
await idle();
const before = await dims();

await page.getByTestId("cat-enhance").first().click();
await page.locator('[data-testid="enhance-upscale"] button', { hasText: "2×" }).click();
await page.waitForFunction(() => !document.querySelector('[data-testid="enhance-upscale"] .lt-spin'), null, { timeout: 300_000 });
await idle();

const errs = await errors();
check(errs.length === 0, `Upscale after Adjust shows no error${errs.length ? ` (${errs.join(" | ")})` : ""}`);
const after = await dims();
check(after.w === before.w * 2 && after.h === before.h * 2, `Upscale 2× doubled the photo (${before.w}×${before.h} -> ${after.w}×${after.h})`);

await browser.close();
console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
