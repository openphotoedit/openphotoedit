// The Lite profile end to end, on a real photo, at desktop and phone sizes.
//   scripts/build-wasm.sh --dev
//   cd apps/web && npx vite --port 5219 --strictPort
//   node apps/web/e2e/lite.spec.mjs [desktop|phone]
// Screenshots land in target/lite/out/. Fails (exit 1) on any broken step.
import { chromium } from "playwright";
import { mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5219/";
const PHOTO = process.env.PHOTO ?? resolve(root, "testdata/photos/portrait.jpg");
const OUT = resolve(root, "target/lite/out");
mkdirSync(OUT, { recursive: true });
const only = process.argv[2];
const SCHEME = process.env.SCHEME ?? "light";

const browser = await chromium.launch({ headless: !process.env.HEADED });
let failures = 0;

function check(ok, what) {
  if (ok) console.log(`  ok   ${what}`);
  else {
    failures++;
    console.log(`  FAIL ${what}`);
  }
}

async function run(name, contextOptions) {
  console.log(`\n${name}`);
  const phone = !!contextOptions.hasTouch;
  const context = await browser.newContext({ ...contextOptions, colorScheme: SCHEME, acceptDownloads: true });
  // The File System Access picker is a native dialog; use the download path.
  await context.addInitScript(() => {
    try {
      delete window.showSaveFilePicker;
    } catch {}
    window.showSaveFilePicker = undefined;
  });
  const page = await context.newPage();
  const logs = [];
  page.on("console", (m) => {
    if (m.type() === "error") logs.push(`${m.type()}: ${m.text()}`);
    if (process.env.DEBUG) console.log("   [console]", m.type(), m.text());
  });
  page.on("pageerror", (e) => logs.push(`pageerror: ${e.message}`));
  // Several sessions edit this checkout; a dev-server reload mid-run is not a Lite failure.
  let navigations = 0;
  page.on("framenavigated", (f) => {
    if (f === page.mainFrame() && ++navigations > 2) console.log("  note: the dev server reloaded the page (a file changed); rerun if a step below fails");
  });
  const shot = async (n) => {
    await page.waitForTimeout(250);
    await page.screenshot({ path: resolve(OUT, `${name}-${n}.png`) });
  };
  const press = (loc) => (phone ? loc.tap() : loc.click());
  const idle = () => page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'), null, { timeout: 60_000 });

  await page.goto(URL_);
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await press(page.getByTestId("choose-lite"));
  await page.getByTestId("lite-start").waitFor();
  await shot("00-start");

  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), press(page.getByTestId("open"))]);
  await chooser.setFiles(PHOTO);
  await page.getByTestId("lite").waitFor({ timeout: 30_000 });
  await idle();
  await page.waitForTimeout(800);
  await shot("01-opened");

  const canvasBox = async () => page.getByTestId("canvas").boundingBox();

  // Adjust: Light and Color.
  await press(page.getByTestId("cat-adjust").first());
  if (phone) {
    const expanded = await page.locator(".sheet__head").getAttribute("aria-expanded");
    if (expanded !== "true") await page.locator(".sheet__head").tap();
  }
  await page.getByTestId("slider-light").fill("45");
  await page.waitForTimeout(400);
  await page.getByTestId("slider-color").fill("35");
  await page.waitForTimeout(600);
  const lightValue = await page.getByTestId("slider-light").inputValue();
  check(Number(lightValue) > 30, `Light slider holds its value (${lightValue})`);
  await press(page.locator('[data-testid="group-light"] .disclose'));
  await page.waitForTimeout(200);
  const exposure = Number(await page.getByTestId("slider-exposure").inputValue());
  check(exposure > 0.1, `Light lifted exposure (${exposure})`);
  await shot("02-adjust");

  // Auto: four engine-rendered alternatives.
  await press(page.getByTestId("cat-auto").first());
  await page.locator('[data-testid="auto-vivid"] img').waitFor({ timeout: 30_000 });
  await page.waitForTimeout(1200);
  await shot("02b-auto");

  // Describe an edit: plan, review, apply.
  const input = page.getByTestId("prompt-input");
  await press(input);
  await input.fill("make it warmer and brighter");
  await page.keyboard.press("Enter");
  await page.getByTestId("prompt-apply").waitFor({ timeout: 20_000 });
  await shot("02c-plan");
  const before = await page.evaluate(() => document.querySelectorAll('[data-testid="prompt"]').length);
  await press(page.getByTestId("prompt-apply"));
  await page.waitForTimeout(1000);
  check(before > 0 && (await page.getByTestId("prompt-apply").count()) === 0, "Planned edit applied");

  // Effects: a Look with strength.
  await press(page.getByTestId("cat-effects").first());
  await page.locator('[data-testid="look-film"] img').waitFor({ timeout: 30_000 });
  await page.waitForTimeout(1500);
  await shot("03-looks");
  await press(page.getByTestId("look-film"));
  await page.getByTestId("look-strength").waitFor({ timeout: 10_000 });
  await page.getByTestId("look-strength").fill("60");
  await page.waitForTimeout(800);
  const strength = await page.getByTestId("look-strength").inputValue();
  check(Math.abs(Number(strength) - 60) <= 1, `Look strength is 60% (${strength})`);
  check(await page.locator('[data-testid="look-film"][aria-pressed="true"]').count() === 1, "Film look is selected");
  await shot("04-look-strength");

  // Crop to 4:5.
  await press(page.getByTestId("cat-crop").first());
  await page.waitForTimeout(300);
  await press(page.getByTestId("ratio-4:5"));
  await page.waitForTimeout(400);
  await shot("05-crop-frame");
  await press(page.getByTestId("crop-apply"));
  await page.waitForTimeout(1200);
  await idle();
  const dims = await page.evaluate(() => {
    const el = document.querySelector('[data-testid="crop-dims"]');
    return el?.textContent ?? "";
  });
  const [w, h] = dims.split("×").map((s) => Number(s.trim()));
  check(w > 0 && Math.abs(w / h - 0.8) < 0.01, `Cropped to 4:5 (${dims})`);
  await shot("06-cropped");

  // Retouch and Enhance, for the record.
  await press(page.getByTestId("cat-retouch").first());
  await page.waitForTimeout(400);
  await shot("06b-retouch");
  await press(page.getByTestId("cat-enhance").first());
  await page.waitForTimeout(400);
  await shot("06c-enhance");

  // Markup: an arrow and some text.
  await press(page.getByTestId("cat-markup").first());
  await press(page.getByTestId("markup-arrow"));
  let box = await canvasBox();
  const pt = (fx, fy) => ({ x: box.x + box.width * fx, y: box.y + box.height * fy });
  const a = pt(0.3, 0.35);
  const b = pt(0.55, 0.55);
  if (phone) {
    const cdp = await context.newCDPSession(page);
    const touch = (type, p) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: p ? [{ x: p.x, y: p.y }] : [] });
    await touch("touchStart", a);
    for (let i = 1; i <= 12; i++) await touch("touchMove", { x: a.x + ((b.x - a.x) * i) / 12, y: a.y + ((b.y - a.y) * i) / 12 });
    await touch("touchEnd");
  } else {
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    for (let i = 1; i <= 12; i++) await page.mouse.move(a.x + ((b.x - a.x) * i) / 12, a.y + ((b.y - a.y) * i) / 12);
    await page.mouse.up();
  }
  await page.waitForTimeout(700);
  await shot("07-arrow");
  await press(page.getByTestId("markup-text"));
  box = await canvasBox();
  const tp = pt(0.25, 0.72);
  if (phone) await page.touchscreen.tap(tp.x, tp.y);
  else await page.mouse.click(tp.x, tp.y);
  await page.waitForTimeout(400);
  await page.keyboard.type("Hello from Lite");
  await page.waitForTimeout(200);
  await shot("08-typing");
  const done = page.getByTestId("text-done");
  if (await done.count()) await press(done);
  else await page.keyboard.press("Escape");
  await page.waitForTimeout(800);
  await shot("09-markup");

  // Steps: go back one.
  if (phone) {
    await press(page.getByTestId("more"));
    await press(page.getByTestId("menu-steps"));
  } else {
    await press(page.getByTestId("steps-toggle"));
  }
  await page.getByTestId("steps").waitFor();
  const stepCount = await page.locator('[data-testid^="step-"]').count();
  check(stepCount >= 5, `Steps lists the edits (${stepCount - 1} steps)`);
  await shot("10-steps");
  const texts = await page.locator('[data-testid^="step-"] .text').allTextContents();
  console.log(`       steps: ${texts.join(" · ")}`);
  await press(page.getByTestId(`step-${stepCount - 2}`));
  await page.waitForTimeout(800);
  const future = await page.locator(".row.future").count();
  check(future === 1, `Went back one step (${future} step ahead)`);
  await shot("11-back-one");
  if (phone) await press(page.getByTestId("sheet-head"));
  else await press(page.getByTestId("steps-toggle"));

  // Export: JPEG under 500 KB.
  await press(page.getByTestId("export"));
  await page.getByTestId("export-sheet").waitFor();
  await press(page.getByTestId("format-jpeg"));
  await press(page.getByTestId("limit-500"));
  await page.waitForFunction(() => /KB|MB/.test(document.querySelector('[data-testid="export-estimate"]')?.textContent ?? ""), null, { timeout: 60_000 });
  await shot("12-export");
  const [download] = await Promise.all([page.waitForEvent("download", { timeout: 60_000 }), press(page.getByTestId("export-save"))]);
  const file = resolve(OUT, `${name}-export.jpg`);
  await download.saveAs(file);
  const bytes = statSync(file).size;
  check(bytes > 20_000 && bytes <= 500_000, `Exported JPEG is under 500 KB (${Math.round(bytes / 1000)} KB)`);
  await page.waitForTimeout(500);
  await shot("13-exported");

  // Tool search.
  await page.keyboard.press(process.platform === "darwin" ? "Meta+K" : "Control+K");
  await page.getByTestId("tool-search").waitFor();
  await page.keyboard.type("warm");
  await page.waitForTimeout(200);
  await shot("14-search");
  await page.keyboard.press("Escape");

  const bad = logs.filter((l) => !/favicon|Failed to load resource/.test(l));
  check(bad.length === 0, `No console errors${bad.length ? `: ${bad.slice(0, 5).join(" | ")}` : ""}`);
  await context.close();
}

if (!only || only === "desktop") await run("desktop", { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
if (!only || only === "phone")
  await run("phone", { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true, userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1" });

await browser.close();
console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
