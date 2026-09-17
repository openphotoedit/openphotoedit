// The screenshots the published test guide embeds (docs/artifacts/test-guide/img).
// This script drives the real app through the same flows as lite.spec.mjs,
// pro.spec.mjs and tools.spec.mjs and stops at the states the guide's captions
// describe. Run it against the production build:
//   cd apps/web && npx vite build && npx vite preview --port 5203 --strictPort
//   node apps/web/e2e/guide-shots.mjs [outDir]        # default target/ship/guide-shots
//
// It produces, at 1440×900 (deviceScaleFactor 2) unless noted:
//   02-lite-opened  03-lite-adjust  04-lite-auto  05-lite-plan  06-lite-looks
//   07-lite-crop    08-lite-markup  09-lite-steps 10-lite-export
//   12a-phone-open 12b-phone-adjust 12c-phone-looks 12d-phone-export  (390×844)
//   13-pro-empty 14-pro-curves 15-pro-layer-style 16-pro-gaussian
//   17-pro-smart-filter 18-pro-filter-menu 19-pro-history
// The remaining guide shots come from e2e/capture.mjs (chooser, Lite dark, PSD,
// raw, 24 MP) and the AI/tool specs; run those into their own output dir.
//
// Numbers the captions quote (export size, slider values, step names, look and
// history counts) are written to <outDir>/facts.json so a rerun can be checked
// against the text of the guide.
import { chromium } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const OUT = resolve(process.argv[2] ?? resolve(root, "target/ship/guide-shots"));
const URL_ = process.env.URL ?? "http://localhost:5203/";
const PHOTO = resolve(root, "testdata/photos/portrait.jpg");
mkdirSync(OUT, { recursive: true });
const facts = {};
const MOD = process.platform === "darwin" ? "Meta" : "Control";

const browser = await chromium.launch();

async function fresh(opts) {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, ...opts });
  // The save picker is a native dialog; keep exports on the download path.
  await ctx.addInitScript(() => {
    try {
      delete window.showSaveFilePicker;
    } catch {}
    window.showSaveFilePicker = undefined;
  });
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
const shot = (page, name) => page.screenshot({ path: `${OUT}/${name}.png` });
// A toast over the canvas would ruin the shot; wait for it to go.
const noToast = (page) => page.waitForFunction(() => !document.querySelector(".toast, [data-testid='toast']"), null, { timeout: 15_000 }).catch(() => {});
// Preview tiles shimmer and dim their image until the engine has rendered them.
const tilesRendered = (page, min) =>
  page.waitForFunction(
    (min) => {
      const tiles = [...document.querySelectorAll(".lt-tile")];
      if (tiles.length < min) return false;
      if (document.querySelector(".lt-tile .lt-shimmer, .lt-tile img.dim")) return false;
      const imgs = tiles.map((t) => t.querySelector("img"));
      return imgs.every((i) => i && i.complete && i.naturalWidth > 0);
    },
    min,
    { timeout: 120_000 },
  );

// ── Lite, light theme, desktop: 02 → 10 ───────────────────────────────────────
{
  const { ctx, page } = await fresh({ colorScheme: "light" });
  await page.getByTestId("choose-lite").click();
  await page.getByTestId("lite-start").waitFor();
  await openVia(page, PHOTO, page.getByTestId("open"));
  await page.getByTestId("lite").waitFor({ timeout: 60_000 });
  await idle(page);
  await page.getByTestId("cat-adjust").first().click();
  await page.getByTestId("slider-light").waitFor();
  await noToast(page);
  await page.waitForTimeout(900);
  facts.photoDims = await page.evaluate(() => (document.body.innerText.match(/\d[\d\s]*×[\s\d]*\d/) ?? [""])[0].replace(/\s+/g, " ").trim());
  facts.promptChips = await page.locator('[data-testid="prompt"] .lt-chip').count();
  await shot(page, "02-lite-opened");

  // 03 — Light +45, its sub-sliders showing.
  await page.getByTestId("slider-light").fill("45");
  await page.waitForTimeout(500);
  await page.getByTestId("slider-color").fill("35");
  await page.waitForTimeout(700);
  await page.locator('[data-testid="group-light"] .disclose').click();
  await page.getByTestId("slider-exposure").waitFor();
  await page.waitForTimeout(600);
  facts.light = await page.getByTestId("slider-light").inputValue();
  facts.color = await page.getByTestId("slider-color").inputValue();
  facts.exposure = await page.getByTestId("slider-exposure").inputValue();
  facts.highlights = await page.getByTestId("slider-highlights").inputValue().catch(() => null);
  await noToast(page);
  await shot(page, "03-lite-adjust");

  // 04 — Auto's four engine-rendered versions.
  await page.getByTestId("cat-auto").first().click();
  for (const id of ["auto-auto", "auto-vivid", "auto-natural", "auto-bw"]) await page.locator(`[data-testid="${id}"]`).waitFor({ timeout: 60_000 });
  await tilesRendered(page, 4);
  facts.autoVersions = await page.locator('[data-testid="panel-auto"] .lt-tile .label').allTextContents();
  await page.waitForTimeout(1200);
  await shot(page, "04-lite-auto");

  // 05 — a described edit, reviewable before it is applied.
  const input = page.getByTestId("prompt-input");
  await input.click();
  await input.fill("make it warmer and brighter");
  await page.keyboard.press("Enter");
  await page.getByTestId("prompt-apply").waitFor({ timeout: 30_000 });
  await page.waitForTimeout(600);
  facts.planSteps = (await page.locator('[data-testid="prompt"] .plan .step .label').allTextContents()).map((t) => t.trim());
  await shot(page, "05-lite-plan");
  await page.getByTestId("prompt-apply").click();
  await page.waitForTimeout(1200);
  await idle(page);

  // 06 — the Film look at 60%.
  await page.getByTestId("cat-effects").first().click();
  await page.locator('[data-testid="look-film"] img').waitFor({ timeout: 90_000 });
  await tilesRendered(page, 6);
  facts.looks = await page.locator('.lt-tile[data-testid^="look-"]').count();
  await page.getByTestId("look-film").click();
  await page.getByTestId("look-strength").waitFor({ timeout: 20_000 });
  await page.getByTestId("look-strength").fill("60");
  await page.waitForTimeout(1500);
  await idle(page);
  await tilesRendered(page, 6);
  facts.lookStrength = await page.getByTestId("look-strength").inputValue();
  await noToast(page);
  await shot(page, "06-lite-looks");

  // 07 — a 4:5 crop frame with the resize presets.
  await page.getByTestId("cat-crop").first().click();
  await page.waitForTimeout(400);
  await page.getByTestId("ratio-4:5").click();
  await page.waitForTimeout(700);
  facts.cropDims = (await page.getByTestId("crop-dims").textContent())?.replace(/\s+/g, " ").trim();
  facts.resizePresets = await page.locator('[data-testid^="resize-"]').allTextContents();
  await shot(page, "07-lite-crop");
  await page.getByTestId("crop-apply").click();
  await page.waitForTimeout(1400);
  await idle(page);

  // 08 — an arrow and a text annotation.
  await page.getByTestId("cat-markup").first().click();
  await page.getByTestId("markup-arrow").click();
  const box = await page.getByTestId("canvas").boundingBox();
  const pt = (fx, fy) => ({ x: box.x + box.width * fx, y: box.y + box.height * fy });
  const a = pt(0.3, 0.35);
  const b = pt(0.55, 0.55);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  for (let i = 1; i <= 12; i++) await page.mouse.move(a.x + ((b.x - a.x) * i) / 12, a.y + ((b.y - a.y) * i) / 12);
  await page.mouse.up();
  await page.waitForTimeout(700);
  await page.getByTestId("markup-text").click();
  const tp = pt(0.25, 0.72);
  await page.mouse.click(tp.x, tp.y);
  await page.waitForTimeout(400);
  await page.keyboard.type("Hello from Lite");
  await page.waitForTimeout(300);
  const done = page.getByTestId("text-done");
  if (await done.count()) await done.click();
  else await page.keyboard.press("Escape");
  await page.waitForTimeout(1000);
  await idle(page);
  await noToast(page);
  await shot(page, "08-lite-markup");

  // 09 — the steps that accumulated.
  await page.getByTestId("steps-toggle").click();
  await page.getByTestId("steps").waitFor();
  await page.waitForTimeout(600);
  facts.steps = (await page.locator('[data-testid^="step-"] .text').allTextContents()).map((t) => t.trim());
  await shot(page, "09-lite-steps");
  await page.getByTestId("steps-toggle").click();
  await page.waitForTimeout(400);

  // 10 — export under 500 KB.
  await page.getByTestId("export").click();
  await page.getByTestId("export-sheet").waitFor();
  await page.getByTestId("format-jpeg").click();
  await page.getByTestId("limit-500").click();
  await page.waitForFunction(() => /KB|MB/.test(document.querySelector('[data-testid="export-estimate"]')?.textContent ?? ""), null, { timeout: 90_000 });
  await page.waitForTimeout(800);
  facts.exportEstimate = (await page.getByTestId("export-estimate").textContent())?.replace(/\s+/g, " ").trim();
  await shot(page, "10-lite-export");
  await ctx.close();
}

// ── Lite on a phone: 12a → 12d ────────────────────────────────────────────────
{
  const { ctx, page } = await fresh({
    colorScheme: "light",
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
    userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
  });
  const tap = (loc) => loc.tap();
  const expandSheet = async () => {
    const head = page.locator(".sheet__head");
    if ((await head.getAttribute("aria-expanded")) !== "true") await head.tap();
    await page.waitForTimeout(300);
  };
  await tap(page.getByTestId("choose-lite"));
  await page.getByTestId("lite-start").waitFor();
  await openVia(page, PHOTO, page.getByTestId("open"));
  await page.getByTestId("lite").waitFor({ timeout: 60_000 });
  await idle(page);
  await tap(page.getByTestId("cat-adjust").first());
  await expandSheet();
  await page.getByTestId("slider-light").waitFor();
  await noToast(page);
  await page.waitForTimeout(900);
  await shot(page, "12a-phone-open");

  await page.getByTestId("slider-light").fill("45");
  await page.waitForTimeout(500);
  await page.getByTestId("slider-color").fill("35");
  await page.waitForTimeout(700);
  await tap(page.locator('[data-testid="group-light"] .disclose'));
  await page.getByTestId("slider-exposure").waitFor();
  await page.waitForTimeout(700);
  await noToast(page);
  await shot(page, "12b-phone-adjust");

  // Same described edit as the desktop run, so the export size matches.
  const input = page.getByTestId("prompt-input");
  await tap(input);
  await input.fill("make it warmer and brighter");
  await page.keyboard.press("Enter");
  await page.getByTestId("prompt-apply").waitFor({ timeout: 30_000 });
  await tap(page.getByTestId("prompt-apply"));
  await page.waitForTimeout(1200);
  await idle(page);

  await tap(page.getByTestId("cat-effects").first());
  await expandSheet();
  await page.locator('[data-testid="look-film"] img').waitFor({ timeout: 90_000 });
  await tap(page.getByTestId("look-film"));
  await page.getByTestId("look-strength").waitFor({ timeout: 20_000 });
  await page.getByTestId("look-strength").fill("60");
  await page.waitForTimeout(1800);
  await idle(page);
  await tilesRendered(page, 6);
  await noToast(page);
  await shot(page, "12c-phone-looks");

  await tap(page.getByTestId("cat-crop").first());
  await expandSheet();
  await tap(page.getByTestId("ratio-4:5"));
  await page.waitForTimeout(500);
  await tap(page.getByTestId("crop-apply"));
  await page.waitForTimeout(1400);
  await idle(page);

  await tap(page.getByTestId("cat-markup").first());
  await expandSheet();
  await tap(page.getByTestId("markup-arrow"));
  const box = await page.getByTestId("canvas").boundingBox();
  const pt = (fx, fy) => ({ x: box.x + box.width * fx, y: box.y + box.height * fy });
  const a = pt(0.3, 0.35);
  const b = pt(0.55, 0.55);
  const cdp = await ctx.newCDPSession(page);
  const touch = (type, p) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: p ? [{ x: p.x, y: p.y }] : [] });
  await touch("touchStart", a);
  for (let i = 1; i <= 12; i++) await touch("touchMove", { x: a.x + ((b.x - a.x) * i) / 12, y: a.y + ((b.y - a.y) * i) / 12 });
  await touch("touchEnd");
  await page.waitForTimeout(700);
  await tap(page.getByTestId("markup-text"));
  const tp = pt(0.25, 0.72);
  await page.touchscreen.tap(tp.x, tp.y);
  await page.waitForTimeout(400);
  await page.keyboard.type("Hello from Lite");
  await page.waitForTimeout(300);
  const done = page.getByTestId("text-done");
  if (await done.count()) await tap(done);
  else await page.keyboard.press("Escape");
  await page.waitForTimeout(1000);
  await idle(page);

  await tap(page.getByTestId("export"));
  await page.getByTestId("export-sheet").waitFor();
  await tap(page.getByTestId("format-jpeg"));
  await tap(page.getByTestId("limit-500"));
  await page.waitForFunction(() => /KB|MB/.test(document.querySelector('[data-testid="export-estimate"]')?.textContent ?? ""), null, { timeout: 90_000 });
  await page.waitForTimeout(800);
  facts.phoneExportEstimate = (await page.getByTestId("export-estimate").textContent())?.replace(/\s+/g, " ").trim();
  await shot(page, "12d-phone-export");
  await ctx.close();
}

// ── Pro, dark theme: 13 → 19 ──────────────────────────────────────────────────
{
  const { ctx, page } = await fresh({ colorScheme: "dark" });
  const settle = (ms = 500) => page.waitForTimeout(ms);
  const menuItem = (label) => {
    const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
    return page.locator(".ops-menu .item").filter({ has: page.locator(".label", { hasText: exact }) }).last();
  };
  async function menu(top, ...path) {
    await page.getByTestId(`menu-${top}`).click();
    await page.getByTestId(`menu-${top}-popup`).waitFor();
    for (let i = 0; i < path.length - 1; i++) {
      await menuItem(path[i]).hover();
      await settle(250);
    }
    await menuItem(path[path.length - 1]).click();
  }
  const layerNames = () => page.$$eval('[data-testid="layer-row"]', (rows) => rows.map((r) => r.getAttribute("data-layer-name")));

  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  await page.getByTestId("empty-state").waitFor();
  await settle(900);
  await shot(page, "13-pro-empty");

  // A photo, then a Curves layer with a point pulled up.
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu("file", "Open…")]);
  await chooser.setFiles(PHOTO);
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector('[data-testid="empty-state"]'), null, { timeout: 90_000 });
  await settle(1400);
  await menu("layer", "New Adjustment Layer", "Curves…");
  await page.getByTestId("curve-editor").waitFor();
  await settle(700);
  {
    const curve = page.getByTestId("curve-editor").locator("canvas");
    const cb = await curve.boundingBox();
    const x = cb.x + cb.width * 0.35;
    const y = cb.y + cb.height * 0.65;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x, y - cb.height * 0.18, { steps: 8 });
    await page.mouse.up();
  }
  await settle(1100);
  await idle(page);
  facts.proLayersAfterCurves = await layerNames();
  facts.proStatus = (await page.getByTestId("status-bar").innerText()).replace(/\s+/g, " ").trim();
  await shot(page, "14-pro-curves");

  // Hue/Saturation, visibility, blend mode, reorder, group — the state the
  // Layer Style and filter shots are taken in.
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-hue-saturation").click();
  await page.getByTestId("hue-hue").waitFor();
  {
    const hue = page.getByTestId("hue-hue").locator('[role="slider"]');
    const hb = await hue.boundingBox();
    await page.mouse.click(hb.x + hb.width * 0.72, hb.y + hb.height / 2);
  }
  await settle(800);
  const curvesRow = page.locator('[data-testid="layer-row"][data-layer-name="Curves"]');
  await curvesRow.getByTestId("layer-visibility").click();
  await settle(400);
  await curvesRow.getByTestId("layer-visibility").click();
  await settle(400);
  await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"] .name').click();
  await page.getByTestId("blend-mode").click();
  await page.getByTestId("blend-mode-multiply").click();
  await settle(600);
  {
    const src = await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"]').boundingBox();
    const dst = await page.locator('[data-testid="layer-row"][data-layer-name="Curves"]').boundingBox();
    await page.mouse.move(src.x + 160, src.y + src.height / 2);
    await page.mouse.down();
    await page.mouse.move(src.x + 160, src.y + src.height / 2 + 10, { steps: 4 });
    await page.mouse.move(dst.x + 160, dst.y + dst.height * 0.85, { steps: 10 });
    await page.mouse.up();
  }
  await settle(800);
  await page.locator('[data-testid="layer-row"][data-layer-name="Curves"] .name').click();
  await page.keyboard.down(MOD);
  await page.locator('[data-testid="layer-row"][data-layer-name="Hue/Saturation"] .name').click();
  await page.keyboard.up(MOD);
  await page.getByTestId("new-group").click();
  await settle(800);

  // 15 — the Layer Style dialog and its effect list.
  await page.locator('[data-testid="layer-row"][data-layer-name="Background"] .name').click();
  await page.getByTestId("fx-button").click();
  await menuItem("Drop Shadow…").click();
  await page.getByTestId("layer-style-dialog").waitFor();
  await settle(1200);
  facts.layerStyleEffects = await page.locator('[data-testid="layer-style-dialog"] input[type=checkbox]').count();
  await shot(page, "15-pro-layer-style");
  await page.getByTestId("layer-style-dialog").getByText("Cancel").click();
  await settle(600);

  // 16 — Gaussian Blur previewing on the canvas.
  await menu("filter", "Blur", "Gaussian Blur…");
  await page.getByTestId("filter-dialog").waitFor({ timeout: 10_000 });
  {
    const field = page.getByTestId("filter-param-radius").locator("input");
    await field.fill("12");
    await field.press("Tab");
  }
  await settle(3000);
  await idle(page);
  await shot(page, "16-pro-gaussian");
  await page.getByTestId("dialog-ok").click();
  await page.getByTestId("filter-dialog").waitFor({ state: "detached" });
  await settle(1400);

  // Image Size, so the history has a step to walk back from.
  await menu("image", "Image Size…");
  await page.getByTestId("image-size-dialog").waitFor();
  {
    const w = page.getByTestId("image-size-width").locator("input");
    await w.fill("1200");
    await w.press("Tab");
  }
  await settle(300);
  await page.getByTestId("dialog-ok").click();
  await settle(1800);
  await idle(page);

  // 19 — the History panel with an earlier step clicked.
  await page.getByTestId("strip-history").click();
  await page.getByTestId("history-panel").waitFor();
  await settle(600);
  {
    const rows = page.getByTestId("history-row");
    const n = await rows.count();
    facts.historyRows = await rows.allTextContents();
    await rows.nth(n - 2).click();
  }
  await settle(1800);
  await idle(page);
  facts.proStatusAfterUndo = (await page.getByTestId("status-bar").innerText()).replace(/\s+/g, " ").trim();
  await shot(page, "19-pro-history");
  await page.getByTestId("strip-history").click();
  await settle(500);

  // 17 — a smart object with a Median smart filter under it.
  await page.locator('[data-testid="layer-row"][data-layer-name="Background"] .name').click();
  await menu("filter", "Convert for Smart Filters");
  await settle(1000);
  await menu("filter", "Noise", "Median…");
  await page.getByTestId("filter-dialog").waitFor({ timeout: 10_000 });
  await settle(1800);
  await page.getByTestId("dialog-ok").click();
  await page.getByTestId("filter-dialog").waitFor({ state: "detached" });
  await settle(1600);
  await idle(page);
  await page.getByTestId("smart-filter-row").first().waitFor();
  facts.smartFilters = await page.getByTestId("smart-filter-row").count();
  await shot(page, "17-pro-smart-filter");

  // 18 — the Filter menu open on its groups. (Undo first, as the guide's shot
  // was taken with the smart filter taken back off.)
  await page.locator("[data-testid=canvas]").hover();
  await page.keyboard.press(`${MOD}+z`);
  await settle(1000);
  await idle(page);
  await page.getByTestId("menu-filter").click();
  await page.getByTestId("menu-filter-popup").waitFor();
  await menuItem("Blur").hover();
  await settle(600);
  facts.filterGroups = (await page.locator('[data-testid="menu-filter-popup"] .item .label').allTextContents()).map((t) => t.trim());
  await shot(page, "18-pro-filter-menu");
  await page.keyboard.press("Escape");
  await ctx.close();
}

writeFileSync(`${OUT}/facts.json`, JSON.stringify(facts, null, 1));
console.log(JSON.stringify(facts, null, 1));
await browser.close();
