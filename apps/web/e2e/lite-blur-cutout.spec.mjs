// Blur background after Remove background, in Simple mode, through the
// "Describe an edit" suggestions.
//
// Remove background hides the background with a layer mask and keeps the
// photo's pixels. Blur background used to read the layer through that mask,
// where every background pixel is transparent with black colour, and blurred
// the colour alone: the new layer came out solid black. It must blur the
// photo's own background instead, and never paint black. This reads the
// exported PNG rather than trusting the absence of an error.
//
//   scripts/build-wasm.sh && scripts/fetch-models.sh
//   cd apps/web && npx vite --port 5227 --strictPort
//   node apps/web/e2e/lite-blur-cutout.spec.mjs
// Fails (exit 1) on any broken step. Screenshots land in target/blur/out/.
import { chromium } from "playwright";
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5227/";
const PHOTO = process.env.PHOTO ?? resolve(root, "testdata/photos/portrait.jpg");
const OUT = process.env.OUT ?? resolve(root, "target/blur/out");
mkdirSync(OUT, { recursive: true });

let failures = 0;
function check(ok, what) {
  if (ok) console.log(`  ok   ${what}`);
  else {
    failures++;
    console.log(`  FAIL ${what}`);
  }
}

const browser = await chromium.launch({ headless: !process.env.HEADED });
const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, acceptDownloads: true });
await context.addInitScript(() => {
  window.showSaveFilePicker = undefined;
});
const page = await context.newPage();
const idle = () => page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector(".lt-spin"), null, { timeout: 600_000 });
const shot = (name) => page.screenshot({ path: `${OUT}/${name}.png` });

/** Run a suggestion chip through the plan and its Apply button. */
async function suggestion(label) {
  await page.getByTestId("prompt-input").click();
  await page.locator(".lt-chip", { hasText: new RegExp(`^${label}$`) }).first().click();
  await page.getByTestId("prompt-apply").click({ timeout: 60_000 });
  await page.waitForTimeout(800);
  await idle();
  await page.waitForTimeout(800);
}

/** Mean colour and alpha of a corner patch of the exported PNG. */
async function exportCorner(keepAs) {
  await page.getByTestId("export").click();
  await page.getByTestId("format-png").click();
  const [download] = await Promise.all([page.waitForEvent("download", { timeout: 180_000 }), page.getByTestId("export-save").click()]);
  const file = await download.path();
  if (keepAs) copyFileSync(file, keepAs);
  const png = readFileSync(file).toString("base64");
  await page.keyboard.press("Escape").catch(() => null);
  return page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }), { premultiplyAlpha: "none" });
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const g = c.getContext("2d");
    g.drawImage(bmp, 0, 0);
    const d = g.getImageData(Math.floor(bmp.width * 0.02), Math.floor(bmp.height * 0.02), 40, 40).data;
    const m = [0, 0, 0, 0];
    for (let i = 0; i < d.length; i += 4) for (let k = 0; k < 4; k++) m[k] += d[i + k];
    return m.map((v) => Math.round(v / (d.length / 4)));
  }, png);
}

async function open(file) {
  await page.goto(URL_);
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.getByTestId("choose-lite").click();
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("open").click()]);
  await chooser.setFiles(file);
  await page.getByTestId("lite").waitFor({ timeout: 30_000 });
  await idle();
}

const CUTOUT = resolve(OUT, "cutout.png");

try {
  await open(PHOTO);
  await shot("0-opened");

  await suggestion("Remove background");
  await shot("1-background-removed");
  const cut = await exportCorner(CUTOUT);
  check(cut[3] === 0, `Remove background leaves the corner transparent (rgba ${cut.join(",")})`);

  await suggestion("Blur background");
  await shot("2-background-blurred");
  const errs = (await page.locator(".toast.error").allTextContents()).map((s) => s.trim());
  check(errs.length === 0, `Blur background shows no error${errs.length ? ` (${errs.join(" | ")})` : ""}`);
  const [r, g, b, a] = await exportCorner();
  check(!(a > 0 && r + g + b < 30), `the background is not solid black (rgba ${[r, g, b, a].join(",")})`);
  // The photo's corner is a dark red curtain, about (87, 30, 21).
  check(a === 255 && r > 50 && r > g + 20, `the background is the photo's own background, blurred (rgba ${[r, g, b, a].join(",")})`);

  // A cut-out saved as PNG has no background left at all: the feature says
  // so, and the background stays transparent rather than turning black.
  await open(CUTOUT);
  await suggestion("Blur background");
  await shot("3-cutout-png-blurred");
  const said = (await page.locator(".toast").allTextContents()).join(" | ");
  check(/nothing to blur/i.test(said), `on a transparent PNG it explains there is nothing to blur (${said.trim()})`);
  const t = await exportCorner();
  check(t[3] === 0, `and the background stays transparent (rgba ${t.join(",")})`);
} catch (e) {
  failures++;
  console.log(`  FAIL script ran to the end — ${String(e).split("\n")[0]}`);
  await shot("error").catch(() => null);
}
await browser.close();
console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
