// Layers panel gestures, Photoshop keyboard shortcuts and multiple
// documents (workstream H5): fill keys, eye swipe and Alt-click solo, drag
// rows onto another document's tab, Alt-drag duplicate, multi-row drag,
// Alt-click clipping, blend hover preview, opacity digits, blend stepping,
// the Cmd+H / Shift+Cmd+E meanings, Cmd-click masks and thumbnails, View ›
// Snap and Filter › Liquify.
//
//   scripts/build-wasm.sh --dev
//   cd apps/web && npx vite --port 5245 --strictPort
//   node apps/web/e2e/h5-layers.spec.mjs
//
// Screenshots land in target/h5/out/. Exits non-zero if any check failed.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL = process.env.URL ?? "http://localhost:5245/";
const PHOTO = resolve(root, "testdata/photos/portrait.jpg");
const OUT = resolve(root, "target/h5/out");
mkdirSync(OUT, { recursive: true });

let failed = false;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failed = true;
}

const browser = await chromium.launch();
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })).newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const MOD = process.platform === "darwin" ? "Meta" : "Control";
const settle = (ms = 400) => page.waitForTimeout(ms);
const shot = (n) => page.screenshot({ path: `${OUT}/${n}.png` });

// The dev server stamps edited modules with ?t=…; import the instance the app
// actually loaded, not a fresh copy.
await page.addInitScript(() => {
  window.__imp = (path) => {
    const hit = performance.getEntriesByType("resource").map((e) => e.name).find((n) => new URL(n).pathname === path);
    return import(hit ?? path);
  };
});
/** Run code with `editor` (and `a`) in the page; returns its result. */
const ed = (src, a) =>
  page.evaluate(
    async ([src, a]) => {
      const { editor } = await window.__imp("/src/lib/editor.svelte.ts");
      return new Function("editor", "a", `return (async () => { ${src} })()`)(editor, a);
    },
    [src, a],
  );
const undoLen = () => ed("return editor.summary.history.undo.length");
const lastLabel = () => ed("return editor.summary.history.undo.at(-1)");
const rows = () => page.locator('[data-testid="layer-row"]');
const rowNames = () => page.$$eval('[data-testid="layer-row"]', (r) => r.map((x) => x.getAttribute("data-layer-name")));
const eyes = () => page.$$eval('[data-testid="layer-visibility"]', (b) => b.map((x) => x.getAttribute("aria-pressed") === "true"));
const activeLayer = () => ed("const find=(l,id)=>{for(const x of l){if(x.id===id)return x;const c=x.children&&find(x.children,id);if(c)return c}return null};return find(editor.summary.layers, editor.summary.active)");
const blur = () => page.evaluate(() => document.activeElement?.blur());
const px = (id, x, y) => ed("return Array.from(await editor.engine.call('layer_region', a[0], a[1], a[2], 1, 1))", [id, x, y]);
const merged = (x, y) => ed("return Array.from(await editor.engine.call('region', a[0], a[1], 1, 1))", [x, y]);

async function menu(top, testid) {
  await page.getByTestId(`menu-${top}`).click();
  await page.getByTestId(`menu-${top}-popup`).waitFor();
  return page.getByTestId(testid);
}

async function newLayer(name) {
  await ed("const ops = await window.__imp('/src/pro/layer-ops.ts'); await ops.newLayer();");
  await settle(200);
  if (name) await ed("await editor.exec({ op: 'layer.props', id: editor.summary.active, name: a })", name);
  await settle(200);
}

async function dragRow(from, to, opts = {}) {
  const a = await from.boundingBox();
  const b = await to.boundingBox();
  if (opts.alt) await page.keyboard.down("Alt");
  await page.mouse.move(a.x + 150, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + 150, a.y + a.height / 2 + 8, { steps: 3 });
  await page.mouse.move(b.x + (opts.x ?? 150), b.y + b.height * (opts.f ?? 0.85), { steps: 10 });
  await settle(150);
  if (opts.shot) await shot(opts.shot);
  await page.mouse.up();
  if (opts.alt) await page.keyboard.up("Alt");
  await settle(500);
}

try {
  await page.goto(URL);
  await page.evaluate(() => localStorage.clear());
  await page.goto(URL);
  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  await settle(600);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("menu-file").click().then(() => page.getByTestId("action-file.open").click())]);
  await chooser.setFiles(PHOTO);
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector('[data-testid="empty-state"]'));
  await settle(1000);
  const W = await ed("return editor.summary.width");
  const H = await ed("return editor.summary.height");
  const bg = await ed("return editor.summary.active");

  // ------------------------------------------------------------------ 1. Fill keys
  await blur();
  await ed("editor.primary = { r: 255, g: 0, b: 0, a: 255 }; editor.secondary = { r: 0, g: 0, b: 255, a: 255 };");
  await page.keyboard.press(`${MOD}+a`);
  await settle(300);
  let n = await undoLen();
  await page.keyboard.press("Alt+Backspace");
  await settle(500);
  let c = await px(bg, W >> 1, H >> 1);
  check("Alt+Backspace fills the selection with the foreground colour", c.join() === "255,0,0,255" && (await undoLen()) === n + 1 && (await lastLabel()) === "Fill", `${c} ${await lastLabel()}`);
  await shot("01-fill-foreground");
  await page.keyboard.press(`${MOD}+Backspace`);
  await settle(500);
  c = await px(bg, 10, 10);
  check("Mod+Backspace fills with the background colour", c.join() === "0,0,255,255", String(c));
  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+z`);
  await settle(500);
  c = await px(bg, W >> 1, H >> 1);
  check("Two undos restore the photo", c.join() !== "255,0,0,255" && c.join() !== "0,0,255,255", String(c));

  // Preserve transparency: a layer with a small opaque square.
  await newLayer("Square");
  const sq = await ed("return editor.summary.active");
  await ed("await editor.exec({ op: 'select.rect', x: 100, y: 100, width: 50, height: 50 })");
  await blur();
  await page.keyboard.press("Alt+Backspace");
  await settle(300);
  await page.keyboard.press(`${MOD}+a`);
  await ed("editor.primary = { r: 0, g: 200, b: 0, a: 255 }");
  n = await undoLen();
  await page.keyboard.press("Shift+Alt+Backspace");
  await settle(600);
  const inside = await px(sq, 120, 120);
  const outside = await px(sq, 300, 300);
  const lockAfter = await ed("const f=(l)=>{for(const x of l){if(x.id===a)return x}};return f(editor.summary.layers).locks.transparency", sq);
  check(
    "Shift+Alt+Backspace fills but keeps transparent pixels transparent, in one step",
    inside.join() === "0,200,0,255" && outside[3] === 0 && (await undoLen()) === n + 1 && (await lastLabel()) === "Fill" && lockAfter === false,
    `inside ${inside} outside ${outside} steps ${(await undoLen()) - n} lock ${lockAfter}`,
  );
  await page.keyboard.press("Shift+Backspace");
  await settle(400);
  check("Shift+Backspace opens the Fill dialog", await page.getByTestId("fill-dialog").isVisible());
  await shot("02-fill-dialog");
  await page.keyboard.press("Escape");
  await settle(300);
  await page.keyboard.press(`${MOD}+d`);
  await settle(300);

  // ------------------------------------------------------------------ 2. Eye swipe and Alt-click solo
  await newLayer("L2");
  await newLayer("L3");
  const eyeBtns = page.locator('[data-testid="layer-visibility"]');
  const before = await eyes();
  n = await undoLen();
  const e0 = await eyeBtns.nth(0).boundingBox();
  const e2 = await eyeBtns.nth(2).boundingBox();
  await page.mouse.move(e0.x + e0.width / 2, e0.y + e0.height / 2);
  await page.mouse.down();
  await page.mouse.move(e0.x + e0.width / 2, e2.y + e2.height / 2, { steps: 12 });
  await page.mouse.up();
  await settle(600);
  let after = await eyes();
  check("Eye swipe hides the three rows it crosses", JSON.stringify(after) === JSON.stringify([false, false, false, true]), `${JSON.stringify(before)} -> ${JSON.stringify(after)}`);
  check("The whole swipe is one undo step", (await undoLen()) === n + 1 && (await lastLabel()) === "Hide Layers", `${(await undoLen()) - n} ${await lastLabel()}`);
  await shot("03-eye-swipe");
  await page.keyboard.press(`${MOD}+z`);
  await settle(500);
  check("One undo shows them all again", (await eyes()).every(Boolean), JSON.stringify(await eyes()));
  await eyeBtns.nth(1).click();
  await settle(400);
  check("A plain click still toggles one eye", JSON.stringify(await eyes()) === JSON.stringify([true, false, true, true]));
  await eyeBtns.nth(1).click();
  await settle(400);
  n = await undoLen();
  await eyeBtns.nth(1).click({ modifiers: ["Alt"] });
  await settle(600);
  check("Alt-click an eye shows only that layer", JSON.stringify(await eyes()) === JSON.stringify([false, true, false, false]) && (await undoLen()) === n + 1, JSON.stringify(await eyes()));
  await shot("04-eye-solo");
  await eyeBtns.nth(1).click({ modifiers: ["Alt"] });
  await settle(600);
  check("Alt-click it again brings the others back", (await eyes()).every(Boolean), JSON.stringify(await eyes()));

  // ------------------------------------------------------------------ 3. Row gestures
  // Rows now (top to bottom): L3, L2, Square, Background.
  let names = await rowNames();
  n = await undoLen();
  await dragRow(rows().nth(0), rows().nth(2), { alt: true, shot: "05-alt-drag" });
  let names2 = await rowNames();
  check("Alt-drag a row duplicates it where it is dropped; the original stays", names2.length === names.length + 1 && names2[0] === names[0] && names2.includes(`${names[0]} copy`) && (await undoLen()) === n + 1, `${JSON.stringify(names)} -> ${JSON.stringify(names2)}`);
  await page.keyboard.press(`${MOD}+z`);
  await settle(400);
  // Select the top two rows and drag both below Square.
  await rows().nth(0).click({ position: { x: 160, y: 18 } });
  await rows().nth(1).click({ position: { x: 160, y: 18 }, modifiers: ["Shift"] });
  await settle(300);
  n = await undoLen();
  await dragRow(rows().nth(0), rows().nth(2), { shot: "06-multi-drag" });
  names2 = await rowNames();
  const selectedAfter = await page.$$eval('[data-testid="layer-row"][aria-selected="true"]', (r) => r.map((x) => x.getAttribute("data-layer-name")));
  check(
    "Dragging two selected rows moves both, in order, as one step",
    JSON.stringify(names2) === JSON.stringify(["Square", "L3", "L2", names.at(-1)]) && (await undoLen()) === n + 1 && JSON.stringify(selectedAfter) === JSON.stringify(["L3", "L2"]),
    `${JSON.stringify(names2)} steps ${(await undoLen()) - n} selected ${JSON.stringify(selectedAfter)}`,
  );
  await page.keyboard.press(`${MOD}+z`);
  await settle(400);
  // Alt-click the line under the top row: clip it to the row below.
  const r0 = await rows().nth(0).boundingBox();
  await page.keyboard.down("Alt");
  await page.mouse.move(r0.x + 170, r0.y + r0.height * 0.92);
  await settle(150);
  const cursor = await rows().nth(0).evaluate((el) => getComputedStyle(el).cursor);
  await page.mouse.click(r0.x + 170, r0.y + r0.height * 0.92);
  await page.keyboard.up("Alt");
  await settle(400);
  let top = await ed("return editor.summary.layers.at(-1)");
  check("Alt-click between two rows clips the upper one", top.clip === true && (await lastLabel()) === "Create Clipping Mask", `clip ${top.clip}, cursor ${cursor.slice(0, 40)}`);
  await shot("07-alt-clip");
  await page.keyboard.down("Alt");
  await page.mouse.click(r0.x + 170, r0.y + r0.height * 0.92);
  await page.keyboard.up("Alt");
  await settle(400);
  top = await ed("return editor.summary.layers.at(-1)");
  check("Alt-click it again releases the clip", top.clip === false);

  // Blend hover preview on the Square layer (green over the photo).
  await page.locator('[data-testid="layer-row"][data-layer-name="Square"] .name').click();
  await settle(300);
  const pixel0 = await merged(120, 120);
  n = await undoLen();
  await page.getByTestId("blend-mode").click();
  await page.getByTestId("blend-mode-multiply").hover();
  await settle(700);
  const pixelHover = await merged(120, 120);
  const blendHover = (await activeLayer()).blend;
  await shot("08-blend-hover");
  await page.keyboard.press("Escape");
  await settle(600);
  const pixelBack = await merged(120, 120);
  check("Hovering Multiply previews it on the canvas", blendHover === "multiply" && pixelHover.join() !== pixel0.join(), `${pixel0} -> ${pixelHover}`);
  check("Escape puts the blend mode and History back", (await activeLayer()).blend === "normal" && pixelBack.join() === pixel0.join() && (await undoLen()) === n, `${pixelBack} steps ${(await undoLen()) - n}`);
  await page.getByTestId("blend-mode").click();
  await page.getByTestId("blend-mode-multiply").hover();
  await settle(300);
  await page.getByTestId("blend-mode-screen").hover();
  await settle(300);
  await page.getByTestId("blend-mode-screen").click();
  await settle(600);
  check("Choosing Screen after hovering others is one Blend Mode step", (await activeLayer()).blend === "screen" && (await undoLen()) === n + 1 && (await lastLabel()) === "Blend Mode", `${(await activeLayer()).blend} ${(await undoLen()) - n}`);
  await page.keyboard.press(`${MOD}+z`);
  await settle(400);

  // ------------------------------------------------------------------ 4. Shortcuts
  await blur();
  await page.keyboard.press("v");
  n = await undoLen();
  await page.keyboard.press("4");
  await page.keyboard.press("5");
  await settle(500);
  check("Move tool: 4 then 5 sets the layer opacity to 45% in one step", Math.abs((await activeLayer()).opacity - 0.45) < 1e-6 && (await undoLen()) === n + 1, `${(await activeLayer()).opacity}`);
  await settle(700);
  await page.keyboard.press("7");
  await settle(500);
  check("A single 7 after a pause sets 70%", Math.abs((await activeLayer()).opacity - 0.7) < 1e-6, `${(await activeLayer()).opacity}`);
  await page.keyboard.press("0");
  await settle(900);
  await page.keyboard.press("Shift+Equal");
  await settle(400);
  check("Shift+Plus steps to the next blend mode", (await activeLayer()).blend === "dissolve", (await activeLayer()).blend);
  await page.keyboard.press("Shift+Minus");
  await page.keyboard.press("Shift+Minus");
  await settle(500);
  check("Shift+Minus twice wraps to the last mode", (await activeLayer()).blend === "luminosity", (await activeLayer()).blend);
  await shot("09-blend-step");
  const extras0 = await page.evaluate(async () => (await window.__imp("/src/pro/state.svelte.ts")).pro.layout.extras);
  await page.keyboard.press(`${MOD}+h`);
  await settle(300);
  const extras1 = await page.evaluate(async () => (await window.__imp("/src/pro/state.svelte.ts")).pro.layout.extras);
  check("Mod+H toggles View › Extras (Photoshop's meaning)", extras0 !== extras1, `${extras0} -> ${extras1}`);
  await page.keyboard.press(`${MOD}+h`);
  const layersBefore = (await rowNames()).length;
  await page.keyboard.press(`Shift+${MOD}+e`);
  await settle(600);
  check("Shift+Mod+E merges visible layers (Photoshop's meaning)", (await lastLabel()) === "Merge Visible" && (await rowNames()).length < layersBefore, `${await lastLabel()} ${layersBefore} -> ${(await rowNames()).length}`);
  await page.keyboard.press(`${MOD}+z`);
  await settle(500);

  // ------------------------------------------------------------------ 5. Cmd-click thumbnails and masks
  await ed("await editor.exec({ op: 'select.rect', x: 40, y: 60, width: 200, height: 120 })");
  await page.locator('[data-testid="layer-row"][data-layer-name="L2"] .name').click();
  await ed("await editor.exec({ op: 'layer.add-mask', id: editor.summary.active, from: 'selection' })");
  await ed("await editor.exec({ op: 'select.none' })");
  await settle(400);
  const maskThumb = page.locator('[data-testid="layer-row"][data-layer-name="L2"] [data-testid="mask-thumb"]');
  await maskThumb.click({ modifiers: [MOD] });
  await settle(500);
  let sel = await ed("return editor.summary.selection?.bounds");
  check("Mod-click the mask thumbnail loads the mask as a selection", sel && sel.x === 40 && sel.y === 60 && sel.w === 200 && sel.h === 120, JSON.stringify(sel));
  await shot("10-mask-selection");
  const sqThumb = page.locator('[data-testid="layer-row"][data-layer-name="Square"] .thumb-btn').first();
  await sqThumb.click({ modifiers: [MOD, "Shift"] });
  await settle(500);
  sel = await ed("return editor.summary.selection?.bounds");
  check("Mod+Shift-click a layer thumbnail adds its pixels", sel && sel.x === 40 && sel.y === 60 && sel.w === 200 && sel.h === 120 && (await ed("return Array.from(await editor.engine.call('selection_region', 120, 170, 1, 1))"))[0] === 255, JSON.stringify(sel));
  await sqThumb.click({ modifiers: [MOD, "Alt"] });
  await settle(500);
  const cut = await ed("return Array.from(await editor.engine.call('selection_region', 120, 120, 1, 1))");
  check("Mod+Alt-click a layer thumbnail subtracts its pixels", cut[0] === 0, String(cut));
  await ed("await editor.exec({ op: 'select.none' })");

  // ------------------------------------------------------------------ 6. View › Snap and Filter › Liquify
  const snapItem = await menu("view", "action-view.snap");
  const snapChecked = await snapItem.getAttribute("aria-checked");
  await shot("11-view-snap");
  await snapItem.click();
  await settle(300);
  const snapState = await page.evaluate(async () => {
    const s = await window.__imp("/src/lib/snap.svelte.ts");
    let tool = null;
    try {
      tool = (await window.__imp("/src/tools/snap.ts")).snapConfig.enabled;
    } catch {}
    return { store: s.snap.enabled, tool, stored: localStorage.getItem("ops.snap") };
  });
  check("View › Snap turns snapping off for the store and the snapping tools", snapChecked === "true" && snapState.store === false && snapState.tool !== true && snapState.stored === "off", JSON.stringify(snapState));
  await (await menu("view", "action-view.snap")).click();
  const liq = await menu("filter", "action-filter.liquify");
  const hasLiquify = await page.evaluate(async () => !!(await window.__imp("/src/tools/registry.ts")).TOOLS["liquify"]);
  const liqDisabled = (await liq.getAttribute("aria-disabled")) === "true";
  check("Filter › Liquify is enabled exactly when the liquify tool is registered", liqDisabled === !hasLiquify, `tool ${hasLiquify}, disabled ${liqDisabled}`);
  if (hasLiquify) {
    await liq.click();
    await settle(300);
    check("Filter › Liquify selects the liquify tool", (await ed("return editor.tool")) === "liquify");
    await page.keyboard.press("v");
  } else await page.keyboard.press("Escape");

  // ------------------------------------------------------------------ 7. Drag layers onto another document's tab
  await page.getByTestId("menu-file").click();
  await page.getByTestId("action-file.new").click();
  await page.getByTestId("new-dialog").waitFor();
  await page.getByTestId("dialog-ok").click();
  await settle(800);
  const docs0 = await ed("return JSON.parse(await editor.engine.call('documents'))");
  const second = docs0.find((d) => d.current);
  const first = docs0.find((d) => !d.current);
  await page.getByTestId("doc-tab").nth(0).click();
  await settle(600);
  const srcUndo = await undoLen();
  await page.locator('[data-testid="layer-row"][data-layer-name="Square"] .name').click();
  await page.locator('[data-testid="layer-row"][data-layer-name="L2"] .name').click({ modifiers: [MOD] });
  await settle(300);
  await dragRow(page.locator('[data-testid="layer-row"][data-layer-name="L2"]'), page.getByTestId("doc-tab").nth(1), { f: 0.5, x: 60, shot: "12-drag-to-tab" });
  await settle(600);
  const docs1 = await ed("return JSON.parse(await editor.engine.call('documents'))");
  const second1 = docs1.find((d) => d.id === second.id);
  const first1 = docs1.find((d) => d.id === first.id);
  check("Dropping two rows on the other tab copies them there", second1.layers === second.layers + 2 && second1.current, `${second.layers} -> ${second1.layers}`);
  check("The source document is unchanged", first1.layers === first.layers, `${first.layers} -> ${first1.layers}`);
  const tNames = await rowNames();
  check("The copies keep their names and order, as one Copy Layers step", JSON.stringify(tNames.slice(0, 2)) === JSON.stringify(["L2", "Square"]) && (await lastLabel()) === "Copy Layers" && (await undoLen()) === 1, `${JSON.stringify(tNames)} ${await lastLabel()}`);
  await shot("13-copied-to-tab");
  await page.keyboard.press(`${MOD}+z`);
  await settle(400);
  check("One undo in the target removes the copies", (await rowNames()).length === second.layers);
  await page.getByTestId("doc-tab").nth(0).click();
  await settle(500);
  check("Source history is untouched by the copy", (await undoLen()) === srcUndo);
} catch (e) {
  console.error(e);
  failed = true;
  await shot("99-error").catch(() => {});
} finally {
  if (errors.length) console.log("page errors:", errors);
  await browser.close();
}
process.exit(failed ? 1 : 0);
