// Pro workspace end to end: open a photo through File › Open, add Curves and
// Hue/Saturation adjustment layers and edit them, toggle visibility, change
// a blend mode, reorder by drag, group, run Gaussian Blur through its
// dialog, resize with Image Size, and undo from the History panel.
//
//   scripts/build-wasm.sh --dev
//   cd apps/web && npx vite --port 5218 --strictPort
//   (sibling workstreams rebuilding wasm reload the dev page mid-run; for a
//   stable run: npx vite build --outDir ../../target/pro/dist &&
//   npx vite preview --port 5218 --strictPort --outDir ../../target/pro/dist)
//   node apps/web/e2e/pro.spec.mjs
//
// Screenshots land in target/pro/out/. Exits non-zero on the first failed check.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL = process.env.URL ?? "http://localhost:5218/";
const PHOTO = resolve(root, "testdata/photos/portrait.jpg");
const OUT = resolve(root, "target/pro/out");
mkdirSync(OUT, { recursive: true });

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failed = true;
}
let failed = false;

const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
page.on("console", (m) => {
  if (m.type() === "error") errors.push(`console: ${m.text()}`);
});
const MOD = process.platform === "darwin" ? "Meta" : "Control";

const shot = (name) => page.screenshot({ path: `${OUT}/${name}.png` });
const layerNames = () => page.$$eval('[data-testid="layer-row"]', (rows) => rows.map((r) => r.getAttribute("data-layer-name")));
// Steps on the undo stack (the snapshot row and undone steps excluded).
const historyCount = () => page.$$eval('[data-testid="history-row"]', (rows) => rows.filter((r) => !r.classList.contains("undone")).length);
const statusText = () => page.getByTestId("status-bar").innerText();
const settle = (ms = 500) => page.waitForTimeout(ms);

async function menu(top, ...path) {
  await page.getByTestId(`menu-${top}`).click();
  const popup = page.getByTestId(`menu-${top}-popup`);
  await popup.waitFor();
  for (let i = 0; i < path.length - 1; i++) {
    await menuItem(path[i]).hover();
    await settle(250);
  }
  await menuItem(path[path.length - 1]).click();
}

function menuItem(label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  return page.locator(".ops-menu .item").filter({ has: page.locator(".label", { hasText: exact }) }).last();
}

try {
  await page.goto(URL);
  await page.evaluate(() => localStorage.clear());
  await page.goto(URL);
  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  await settle(600);
  await shot("00-empty");
  check("Pro workspace renders the empty state", await page.getByTestId("empty-state").isVisible());

  // File › Open
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu("file", "Open…")]);
  await chooser.setFiles(PHOTO);
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector('[data-testid="empty-state"]'));
  await settle(1200);
  check("File › Open loads the photo", (await layerNames()).length === 1, JSON.stringify(await layerNames()));
  check("Document tab shows the file name", (await page.getByTestId("doc-tab").first().innerText()).includes("portrait"));
  await shot("01-opened");

  // Curves adjustment layer from Layer › New Adjustment Layer
  await menu("layer", "New Adjustment Layer", "Curves…");
  await page.getByTestId("curve-editor").waitFor();
  await settle(600);
  const curve = page.getByTestId("curve-editor").locator("canvas");
  const box = await curve.boundingBox();
  const before = await curve.screenshot();
  // Grab the diagonal at the quarter-tones and pull it up.
  const x = box.x + box.width * 0.35;
  const y = box.y + box.height * 0.65;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - box.height * 0.18, { steps: 8 });
  await page.mouse.up();
  await settle(900);
  const after = await curve.screenshot();
  check("Curves layer added", (await layerNames())[0] === "Curves", JSON.stringify(await layerNames()));
  check("Dragging on the curve changes it", !before.equals(after));
  await shot("02-curves");

  // Hue/Saturation from the Adjustments panel
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-hue-saturation").click();
  await page.getByTestId("hue-hue").waitFor();
  const hue = page.getByTestId("hue-hue").locator('[role="slider"]');
  const hb = await hue.boundingBox();
  await page.mouse.click(hb.x + hb.width * 0.72, hb.y + hb.height / 2);
  await settle(700);
  check("Hue/Saturation layer added above Curves", (await layerNames())[0] === "Hue/Saturation", JSON.stringify(await layerNames()));
  await shot("03-hue-saturation");

  // Visibility toggle on Curves
  const curvesRow = page.locator('[data-testid="layer-row"][data-layer-name="Curves"]');
  await curvesRow.getByTestId("layer-visibility").click();
  await settle(400);
  check("Visibility toggles off", (await curvesRow.getByTestId("layer-visibility").getAttribute("aria-pressed")) === "false");
  await curvesRow.getByTestId("layer-visibility").click();
  await settle(400);
  check("Visibility toggles back on", (await curvesRow.getByTestId("layer-visibility").getAttribute("aria-pressed")) === "true");

  // Blend mode on Hue/Saturation
  await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"] .name').click();
  await page.getByTestId("blend-mode").click();
  await page.getByTestId("blend-mode-multiply").click();
  await settle(500);
  check("Blend mode changes to Multiply", (await page.getByTestId("blend-mode").getAttribute("data-value")) === "multiply");
  await shot("04-blend-multiply");

  // Reorder by drag: Hue/Saturation below Curves
  const src = await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"]').boundingBox();
  const dst = await page.locator('[data-testid="layer-row"][data-layer-name="Curves"]').boundingBox();
  await page.mouse.move(src.x + 160, src.y + src.height / 2);
  await page.mouse.down();
  await page.mouse.move(src.x + 160, src.y + src.height / 2 + 10, { steps: 4 });
  await page.mouse.move(dst.x + 160, dst.y + dst.height * 0.85, { steps: 10 });
  await shot("05-dragging");
  await page.mouse.up();
  await settle(700);
  const order = await layerNames();
  check("Drag reorders the layers", order[0] === "Curves" && order[1] === "Hue/Saturation", JSON.stringify(order));

  // Group the two adjustment layers
  await page.locator('[data-testid="layer-row"][data-layer-name="Curves"] .name').click();
  await page.keyboard.down(MOD);
  await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"] .name').click();
  await page.keyboard.up(MOD);
  await page.getByTestId("new-group").click();
  await settle(700);
  const kinds = await page.$$eval('[data-testid="layer-row"]', (rows) => rows.map((r) => r.getAttribute("data-kind")));
  check("Selected layers become a group", kinds[0] === "group" && kinds.length === 4, JSON.stringify(kinds));
  await shot("06-group");

  // Layer style dialog on the background
  await page.locator('[data-testid="layer-row"][data-layer-name="Background"] .name').click();
  await page.getByTestId("fx-button").click();
  await menuItem("Drop Shadow…").click();
  await page.getByTestId("layer-style-dialog").waitFor();
  await settle(900);
  await shot("07-layer-style");
  await page.getByTestId("layer-style-dialog").getByText("Cancel").click();
  await settle(500);

  // Gaussian Blur with preview
  const histBefore = await (async () => {
    await page.getByTestId("strip-history").click();
    await settle(300);
    const n = await historyCount();
    await page.getByTestId("strip-history").click();
    return n;
  })();
  try {
    await menu("filter", "Blur", "Gaussian Blur…");
    await page.getByTestId("filter-dialog").waitFor({ timeout: 4000 });
    const field = page.getByTestId("filter-param-radius").locator("input");
    await field.fill("12");
    await field.press("Tab");
    await settle(2500);
    await shot("08-gaussian-preview");
    await page.getByTestId("dialog-ok").click();
    await page.getByTestId("filter-dialog").waitFor({ state: "detached" });
    await settle(1200);
    await page.getByTestId("strip-history").click();
    await settle(300);
    const histAfter = await historyCount();
    check("Gaussian Blur applies as one history step", histAfter === histBefore + 1, `${histBefore} → ${histAfter}`);
    await page.getByTestId("strip-history").click();
  } catch (e) {
    check("Gaussian Blur dialog", false, String(e).split("\n")[0]);
    await page.keyboard.press("Escape");
  }

  // Image Size
  await menu("image", "Image Size…");
  await page.getByTestId("image-size-dialog").waitFor();
  const w = page.getByTestId("image-size-width").locator("input");
  await w.fill("1200");
  await w.press("Tab");
  await settle(200);
  await shot("09-image-size");
  await page.getByTestId("dialog-ok").click();
  await settle(1500);
  const st = await statusText();
  check("Image Size resizes to 1200 px wide", /1200 × 1499 px/.test(st), st.replace(/\s+/g, " "));

  // Undo via the History panel: back to before the resize.
  await page.getByTestId("strip-history").click();
  await page.getByTestId("history-panel").waitFor();
  await settle(400);
  const rows = page.getByTestId("history-row");
  const n = await rows.count();
  await rows.nth(n - 2).click();
  await settle(1500);
  await shot("10-history-undo");
  const st2 = await statusText();
  check("History panel click undoes the resize", /2687 × 3356 px/.test(st2), st2.replace(/\s+/g, " "));
  await page.getByTestId("strip-history").click();

  // Smart filters: convert, then a filter from the menu becomes a smart filter.
  try {
    await page.locator('[data-testid="layer-row"][data-layer-name="Background"] .name').click();
    await menu("filter", "Convert for Smart Filters");
    await settle(900);
    const bgKind = await page.locator('[data-testid="layer-row"]').last().getAttribute("data-kind");
    check("Convert for Smart Filters makes a smart object", bgKind === "smart", String(bgKind));
    await menu("filter", "Noise", "Median…");
    await page.getByTestId("filter-dialog").waitFor({ timeout: 4000 });
    await settle(1500);
    await page.getByTestId("dialog-ok").click();
    await page.getByTestId("filter-dialog").waitFor({ state: "detached" });
    await settle(1200);
    check("A filter on a smart object is listed as a smart filter", (await page.getByTestId("smart-filter-row").count()) === 1);
    await shot("11a-smart-filter");
  } catch (e) {
    check("Smart filter flow", false, String(e).split("\n")[0]);
    await page.keyboard.press("Escape");
  }

  // Keyboard: Cmd/Ctrl+Z undoes; the menu bar opens and runs from the keyboard.
  await page.locator("[data-testid=canvas]").hover();
  await page.keyboard.press(`${MOD}+z`);
  await settle(900);
  check("Undo shortcut takes the smart filter off", (await page.getByTestId("smart-filter-row").count()) === 0);
  await page.getByTestId("menu-help").focus();
  await page.keyboard.press("ArrowDown");
  await page.getByTestId("menu-help-popup").waitFor();
  await page.keyboard.press("Enter");
  const sheet = await page.getByTestId("shortcuts-dialog").waitFor({ timeout: 3000 }).then(() => true, () => false);
  check("Keyboard opens Help › Keyboard Shortcuts", sheet);
  if (sheet) {
    await settle(300);
    await shot("11b-shortcuts");
    await page.keyboard.press("Escape");
    await settle(300);
  }

  // A menu open at full size, and the smaller window.
  await page.getByTestId("menu-filter").click();
  await menuItem("Blur").hover();
  await settle(400);
  await shot("11-filter-menu");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.setViewportSize({ width: 1280, height: 720 });
  await settle(900);
  await shot("12-1280x720");
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight);
  check("No page overflow at 1280×720", !overflow);
  const statusBox = await page.getByTestId("status-bar").boundingBox();
  check("Status bar fully visible at 1280×720", statusBox && statusBox.y + statusBox.height <= 720.5, JSON.stringify(statusBox));
} catch (e) {
  check("script ran to the end", false, String(e).split("\n")[0]);
  await shot("zz-failure").catch(() => {});
} finally {
  const real = errors.filter((e) => !/favicon|onnx|models\//i.test(e));
  if (real.length) console.log(`\nPage errors:\n${real.join("\n")}`);
  await browser.close();
}
console.log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed`);
process.exit(failed ? 1 : 0);
