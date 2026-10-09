// Filter › Liquify behaves as a modal session in Pro: strokes preview on the
// layer, Enter or OK keeps them as ONE history step, Escape or Cancel puts
// the layer back exactly, and switching tool keeps the warp (as Free
// Transform does). Each check reads pixels and the History panel rather than
// trusting that a button exists.
//
//   cd apps/web && npx vite --port 5226 --strictPort
//   node apps/web/e2e/liquify-modal.spec.mjs
// Screenshots land in target/liquify/out/. Exits non-zero on any failed check.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5226/";
const PHOTO = process.env.PHOTO ?? resolve(root, "testdata/photos/portrait.jpg");
const OUT = process.env.OUT ?? resolve(root, "target/liquify/out");
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
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const settle = (ms = 400) => page.waitForTimeout(ms);
const shot = (name) => page.screenshot({ path: `${OUT}/${name}.png` });
// The History panel's live rows (the snapshot row and undone steps excluded).
async function history() {
  await page.getByTestId("strip-history").click();
  await page.getByTestId("history-panel").waitFor();
  await settle(200);
  const rows = await page.$$eval('[data-testid="history-row"]', (rs) => rs.filter((r) => !r.classList.contains("undone")).map((r) => r.textContent.trim()));
  await page.getByTestId("strip-history").click();
  await settle(200);
  return rows;
}
const inLiquify = () => page.getByTestId("liquify-mode").isVisible();

async function menu(top, label) {
  await page.getByTestId(`menu-${top}`).click();
  await page.getByTestId(`menu-${top}-popup`).waitFor();
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".ops-menu .item").filter({ has: page.locator(".label", { hasText: exact }) }).last().click();
}

/** The canvas as RGBA, with the pointer parked off it so no brush ring is drawn. */
async function pixels() {
  await page.mouse.move(4, 4);
  await settle(500);
  const png = (await page.getByTestId("canvas").screenshot()).toString("base64");
  return page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const g = c.getContext("2d");
    g.drawImage(bmp, 0, 0);
    return Array.from(g.getImageData(0, 0, bmp.width, bmp.height).data);
  }, png);
}

function changed(a, b) {
  let n = 0;
  for (let i = 0; i < a.length; i += 4) if (Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1]) + Math.abs(a[i + 2] - b[i + 2]) > 24) n++;
  return n;
}

/** Two forward-warp strokes across the middle of the photo. */
async function strokes() {
  const box = await page.getByTestId("canvas").boundingBox();
  for (const dy of [-0.08, 0.04]) {
    const y = box.y + box.height * (0.42 + dy);
    await page.mouse.move(box.x + box.width * 0.42, y);
    await page.mouse.down();
    for (let i = 1; i <= 16; i++) await page.mouse.move(box.x + box.width * (0.42 + 0.012 * i), y, { steps: 1 });
    await page.mouse.up();
    await settle(300);
  }
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'));
}

try {
  await page.goto(URL_);
  await page.evaluate(() => localStorage.clear());
  await page.goto(URL_);
  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu("file", "Open…")]);
  await chooser.setFiles(PHOTO);
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector('[data-testid="empty-state"]'));
  await settle(1500);
  const original = await pixels();
  const h0 = (await history()).length;

  // 1. Escape discards the whole session.
  await menu("filter", "Liquify…");
  await settle();
  check(await inLiquify(), "Filter › Liquify opens the Liquify options");
  const bar = page.getByTestId("liquify-ok");
  check(await bar.isVisible().catch(() => false), "Liquify shows an OK button");
  check(await page.getByTestId("liquify-cancel").isVisible().catch(() => false), "Liquify shows a Cancel button");
  await strokes();
  const warped = await pixels();
  check(changed(original, warped) > 500, `strokes preview on the canvas (${changed(original, warped)} pixels moved)`);
  await shot("1-liquify-preview");
  await page.keyboard.press("Escape");
  await settle(800);
  const afterEsc = await pixels();
  check(!(await inLiquify()), "Escape leaves Liquify");
  check(changed(original, afterEsc) === 0, `Escape restores the layer exactly (${changed(original, afterEsc)} pixels differ)`);
  check((await history()).length === h0, `Escape leaves no history step (${h0} → ${(await history()).length})`);
  await shot("2-after-escape");

  // 2. Enter keeps it as one step.
  await menu("filter", "Liquify…");
  await settle();
  await strokes();
  await page.keyboard.press("Enter");
  await settle(800);
  const afterEnter = await pixels();
  const h = await history();
  check(!(await inLiquify()), "Enter leaves Liquify");
  check(changed(original, afterEnter) > 500, `Enter keeps the warp (${changed(original, afterEnter)} pixels moved)`);
  check(h.length === h0 + 1 && /Liquify/.test(h.at(-1)), `Enter commits two strokes as one step (${JSON.stringify(h.slice(h0))})`);
  await shot("3-after-enter");
  await page.keyboard.press(process.platform === "darwin" ? "Meta+z" : "Control+z");
  await settle(800);
  check(changed(original, await pixels()) === 0, "one Undo takes the whole Liquify back");

  // 3. The Cancel button discards; the OK button commits.
  await menu("filter", "Liquify…");
  await settle();
  await strokes();
  await page.getByTestId("liquify-cancel").click({ timeout: 3000 }).catch(() => null);
  await settle(800);
  check(changed(original, await pixels()) === 0 && (await history()).length === h0, "Cancel restores the layer and history");
  await menu("filter", "Liquify…");
  await settle();
  await strokes();
  await page.getByTestId("liquify-ok").click({ timeout: 3000 }).catch(() => null);
  await settle(800);
  check(changed(original, await pixels()) > 500 && (await history()).length === h0 + 1, "OK keeps the warp as one step");
  await page.keyboard.press(process.platform === "darwin" ? "Meta+z" : "Control+z");
  await settle(800);

  // 4. Switching tool mid-session keeps the warp, as one step.
  await menu("filter", "Liquify…");
  await settle();
  await strokes();
  await page.keyboard.press("v");
  await settle(800);
  const hs = await history();
  check(!(await inLiquify()), "switching tool leaves Liquify");
  check(changed(original, await pixels()) > 500 && hs.length === h0 + 1 && /Liquify/.test(hs.at(-1)), `switching tool keeps the warp as one step (${JSON.stringify(hs.slice(h0))})`);
  await shot("4-after-tool-switch");
  await page.keyboard.press(process.platform === "darwin" ? "Meta+z" : "Control+z");
  await settle(800);

  // 5. Undo inside the session steps back one stroke and stays in Liquify;
  // OK then keeps what is left as one step.
  await menu("filter", "Liquify…");
  await settle();
  await strokes();
  await page.keyboard.press(process.platform === "darwin" ? "Meta+z" : "Control+z");
  await settle(800);
  check(await inLiquify(), "Undo inside Liquify takes back a stroke and stays in Liquify");
  await page.getByTestId("liquify-ok").click({ timeout: 3000 }).catch(() => null);
  await settle(800);
  const h5 = await history();
  check(changed(original, await pixels()) > 500 && h5.length === h0 + 1, `the stroke left is kept as one step (${JSON.stringify(h5.slice(h0))})`);
} catch (e) {
  failures++;
  console.log(`  FAIL script ran to the end — ${String(e).split("\n")[0]}`);
  await shot("error").catch(() => null);
}
if (errors.length) console.log(errors.join("\n"));
await browser.close();
console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
