// Canvas gestures in the Pro app: right-button handling and the brush scrub,
// move/crop snapping with smart guides, multi-layer transform, numeric
// transform fields, edge autoscroll, drag-to-zoom, selection pixel gestures
// and the lasso's Alt straight segments.
//   cd apps/web && npx vite --port 5244 --strictPort
//   node apps/web/e2e/gestures.spec.mjs [only-test-name]
// Screenshots land in target/gestures/ (or $OUT).
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5244/";
const OUT = process.env.OUT ?? resolve(root, "target/gestures");
mkdirSync(OUT, { recursive: true });
const only = process.argv[2];
const MOD = process.platform === "darwin" ? "Meta" : "Control";

const browser = await chromium.launch();
const results = [];

function check(name, ok, detail) {
  results.push({ name, ok });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail !== undefined ? ` — ${typeof detail === "string" ? detail : JSON.stringify(detail)}` : ""}`);
}

/** Run `src` in the page with `editor`, `mod(path)` (dynamic import) and `a`. */
const ev = (page, src, a) =>
  page.evaluate(
    async ([src, a]) => {
      const { editor } = await import("/src/lib/editor.svelte.ts");
      const mod = (p) => import(p);
      return new Function("editor", "mod", "a", `return (async()=>{${src}})()`)(editor, mod, a);
    },
    [src, a],
  );

const settle = (page, ms = 350) => page.waitForTimeout(ms);

/** A solid RGBA block. */
const block = (w, h, [r, g, b, al = 255]) => `new Uint8Array(${w * h * 4}).map((_, i) => [${r},${g},${b},${al}][i % 4])`;

async function setup({ w = 1000, h = 700 } = {}) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => console.log("pageerror", e.message));
  await page.goto(URL_);
  await page.evaluate(() => localStorage.clear());
  await page.goto(URL_);
  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  for (let i = 0; i < 100 && !(await ev(page, `return editor.ready`)); i++) await settle(page, 100);
  await ev(page, `const io = await mod("/src/lib/io.ts"); await io.newDocument(a.w, a.h, {r:255,g:255,b:255});`, { w, h });
  await settle(page, 600);
  return page;
}

/** Two coloured layers: A red 100×60 at (300,200), B blue 80×80 at (600,400). */
async function twoLayers(page) {
  return ev(
    page,
    `const A = await editor.exec({ op: "layer.import", width: 100, height: 60, x: 300, y: 200, name: "A" }, ${block(100, 60, [220, 30, 30])});
     const B = await editor.exec({ op: "layer.import", width: 80, height: 80, x: 600, y: 400, name: "B" }, ${block(80, 80, [30, 60, 220])});
     await editor.exec({ op: "layer.set-active", id: A.data.id });
     return { A: A.data.id, B: B.data.id };`,
  );
}

async function at(page, x, y) {
  return ev(
    page,
    `const r = document.querySelector('[data-testid="canvas"]').getBoundingClientRect();
     const v = editor.toView(a[0], a[1]);
     return { x: r.left + v.x, y: r.top + v.y };`,
    [x, y],
  );
}

async function dragView(page, a, b, { steps = 14, button = "left", hold, mid } = {}) {
  await page.mouse.move(a.x, a.y);
  await page.mouse.down({ button });
  for (let i = 1; i <= steps; i++) {
    await page.mouse.move(a.x + ((b.x - a.x) * i) / steps, a.y + ((b.y - a.y) * i) / steps);
    if (mid && i === Math.floor(steps / 2)) await mid();
    await page.waitForTimeout(10);
  }
  if (hold) await hold();
  await page.mouse.up({ button });
  await settle(page, 450);
}

const bounds = (page, id) => ev(page, `const all = []; const w = (ls) => ls.forEach((l) => { all.push(l); l.children && w(l.children); }); w(editor.summary.layers); return all.find((l) => l.id === a)?.bounds`, id);
const undoLen = (page) => ev(page, `return editor.summary.history.undo.length`);
const lastUndo = (page) => ev(page, `return editor.summary.history.undo.at(-1)`);
const zoomOf = (page) => ev(page, `return editor.view.zoom`);
const setTool = (page, id) => ev(page, `editor.tool = a`, id);
const setting = (page, k) => ev(page, `return (await mod("/src/tools/settings.svelte.ts")).toolSettings[a]`, k);
const pixel = (page, id, x, y) => ev(page, `return [...await editor.engine.call("layer_region", a[0], a[1], a[2], 1, 1)]`, [id, x, y]);
const selAt = (page, x, y) => ev(page, `return (await editor.engine.call("selection_region", a[0], a[1], 1, 1))[0] ?? 0`, [x, y]);

const tests = {
  async rightButton() {
    const page = await setup();
    await ev(page, `await editor.exec({ op: "layer.add-pixel", name: "Paint" });`);
    await setTool(page, "brush");
    await ev(page, `const s = (await mod("/src/tools/settings.svelte.ts")).toolSettings; s.brushSize = 30; s.brushHardness = 0.8;`);
    await settle(page);
    await page.evaluate(() => {
      window.__ctx = [];
      document.addEventListener("contextmenu", (e) => setTimeout(() => window.__ctx.push(e.defaultPrevented)), true);
    });
    const z = await zoomOf(page);
    const u0 = await undoLen(page);
    const c = await at(page, 500, 350);
    await dragView(page, c, { x: c.x + 120, y: c.y }, {
      button: "right",
      hold: () => page.screenshot({ path: `${OUT}/01-right-drag-size.png` }),
    });
    const size = await setting(page, "brushSize");
    const expect = Math.round(30 + 240 / z);
    check("right-drag: no history step", (await undoLen(page)) === u0, { before: u0, after: await undoLen(page) });
    check("right-drag: size follows the circle edge (30 + 2·dx/zoom)", Math.abs(size - expect) <= 2, { size, expect, zoom: z });
    const active = await ev(page, `return editor.summary.active`);
    check("right-drag: nothing painted", (await pixel(page, active, 500, 350))[3] === 0 && (await pixel(page, active, 560, 350))[3] === 0);
    // Shift + right-drag: hardness, horizontal, 200 px = full range.
    await page.keyboard.down("Shift");
    await dragView(page, c, { x: c.x - 50, y: c.y }, { button: "right", hold: () => page.screenshot({ path: `${OUT}/02-shift-right-drag-hardness.png` }) });
    await page.keyboard.up("Shift");
    const hard = await setting(page, "brushHardness");
    check("shift+right-drag: hardness 0.80 → 0.55, size kept", Math.abs(hard - 0.55) < 0.011 && (await setting(page, "brushSize")) === size, { hard, size: await setting(page, "brushSize") });
    // Photoshop's HUD chord: Ctrl+Alt(+Opt) left-drag, horizontal size, vertical hardness.
    const s0 = await setting(page, "brushSize");
    await page.keyboard.down("Control");
    await page.keyboard.down("Alt");
    await dragView(page, c, { x: c.x + 40, y: c.y - 40 });
    await page.keyboard.up("Alt");
    await page.keyboard.up("Control");
    const s1 = await setting(page, "brushSize");
    const h1 = await setting(page, "brushHardness");
    check("ctrl+alt-drag: size and hardness together", Math.abs(s1 - (s0 + 80 / z)) <= 2 && Math.abs(h1 - 0.75) < 0.011, { s0, s1, h1 });
    check("ctrl+alt-drag: still no history step, nothing painted", (await undoLen(page)) === u0 && (await pixel(page, active, 520, 330))[3] === 0);
    // Right-drag with a non-brush tool does nothing either.
    await setTool(page, "marquee-rect");
    await settle(page);
    await dragView(page, c, { x: c.x + 100, y: c.y + 80 }, { button: "right" });
    check("right-drag with the marquee: no selection, no step", !(await ev(page, `return editor.summary.selection`)) && (await undoLen(page)) === u0);
    const ctx = await page.evaluate(() => window.__ctx);
    check("context menu suppressed on the canvas", ctx.every((p) => p), ctx);
    await page.close();
  },

  async moveSnap() {
    const page = await setup();
    const { A, B } = await twoLayers(page);
    await setTool(page, "move");
    await settle(page);
    const z = await zoomOf(page);
    // Left edge to 3 view px right of the canvas edge: snaps to x = 0.
    const from = await at(page, 350, 230);
    const to = { x: from.x - 300 * z + 3, y: from.y };
    await dragView(page, from, to, { hold: () => page.screenshot({ path: `${OUT}/03-move-snap-canvas-edge.png` }) });
    const b1 = await bounds(page, A);
    check("move snaps the left edge to the canvas edge", b1.x === 0 && b1.y === 200, b1);
    await ev(page, `await editor.undo()`);
    await settle(page);
    // Same drag with Control pressed mid-drag: no snap.
    await dragView(page, from, to, { mid: () => page.keyboard.down("Control") });
    await page.keyboard.up("Control");
    const b2 = await bounds(page, A);
    check("Control disables snapping", b2.x !== 0 && Math.abs(b2.x - 3 / z) <= 1, { b2, want: Math.round(3 / z) });
    await ev(page, `await editor.undo()`);
    await settle(page);
    // Centre to 4 view px from the canvas centre: centres exactly.
    const from2 = await at(page, 350, 230);
    const to2 = { x: from2.x + (500 - 350) * z + 4, y: from2.y + (350 - 230) * z - 3 };
    await dragView(page, from2, to2, { hold: () => page.screenshot({ path: `${OUT}/04-move-snap-centre.png` }) });
    const b3 = await bounds(page, A);
    check("move snaps the centre to the canvas centre", b3.x + b3.w / 2 === 500 && b3.y + b3.h / 2 === 350, b3);
    await ev(page, `await editor.undo()`);
    await settle(page);
    // Right edge next to layer B's left edge (x = 600).
    const to3 = { x: from2.x + 200 * z - 2, y: from2.y + 150 * z };
    await dragView(page, from2, to3, { hold: () => page.screenshot({ path: `${OUT}/05-move-snap-layer-edge.png` }) });
    const b4 = await bounds(page, A);
    check("move snaps to another layer's edge", b4.x + b4.w === 600, b4);
    // View › Snap off (H5's switch) turns it off too.
    await ev(page, `await editor.undo(); (await mod("/src/lib/snap.svelte.ts")).setSnap(false);`);
    await settle(page, 300);
    await dragView(page, from2, to3);
    const b5 = await bounds(page, A);
    check("View › Snap off disables snapping", b5.x + b5.w === 598, b5);
    await ev(page, `(await mod("/src/lib/snap.svelte.ts")).setSnap(true);`);
    void B;
    await page.close();
  },

  async altDuplicateLayer() {
    const page = await setup();
    const { A } = await twoLayers(page);
    await setTool(page, "move");
    await settle(page);
    const z = await zoomOf(page);
    const u0 = await undoLen(page);
    const from = await at(page, 350, 230);
    await page.keyboard.down("Alt");
    await dragView(page, from, { x: from.x + 37 * z, y: from.y + 101 * z });
    await page.keyboard.up("Alt");
    const n = await ev(page, `return editor.summary.layers.length`);
    const act = await ev(page, `return editor.summary.active`);
    const b = await bounds(page, act);
    check("Alt-drag duplicates the layer and moves the copy", n === 4 && act !== A && b.x === 337 && b.y === 301 && (await bounds(page, A)).x === 300, { n, b });
    check("duplicate + move is one undo step", (await undoLen(page)) === u0 + 1, await ev(page, `return editor.summary.history.undo.slice(-2)`));
    await ev(page, `await editor.undo()`);
    await settle(page);
    check("one undo removes the copy", (await ev(page, `return editor.summary.layers.length`)) === 3);
    await page.close();
  },

  async cropTransaction() {
    const page = await setup();
    await setTool(page, "crop");
    await ev(page, `const s = (await mod("/src/tools/settings.svelte.ts")).toolSettings; s.cropOutW = 400; s.cropOutH = 300;`);
    await settle(page);
    const z = await zoomOf(page);
    const u0 = await undoLen(page);
    const e = await at(page, 1000, 700);
    await dragView(page, e, { x: e.x - 200 * z, y: e.y - 200 * z });
    await page.keyboard.press("Enter");
    await settle(page, 800);
    const s = await ev(page, `return { w: editor.summary.width, h: editor.summary.height, undo: editor.summary.history.undo.slice(u => 0) }`);
    check("crop + resize is one undo step", s.w === 400 && s.h === 300 && (await undoLen(page)) === u0 + 1, s);
    await page.close();
  },

  async cropSnap() {
    const page = await setup();
    await twoLayers(page);
    await setTool(page, "crop");
    await settle(page);
    const z = await zoomOf(page);
    // Drag the east handle to 3 view px past layer B's right edge (680).
    const e = await at(page, 1000, 350);
    await dragView(page, e, { x: e.x - 320 * z + 3, y: e.y }, { hold: () => page.screenshot({ path: `${OUT}/06-crop-snap.png` }) });
    const info = await ev(page, `return { ...(await mod("/src/tools/state.svelte.ts")).toolState.cropInfo }`);
    check("crop edge snaps to a layer edge", info.w === 680, info);
    await page.keyboard.press("Escape");
    await settle(page);
    const e2 = await at(page, 1000, 350);
    await dragView(page, e2, { x: e2.x - 320 * z + 3, y: e2.y }, { mid: () => page.keyboard.down("Control") });
    await page.keyboard.up("Control");
    const info2 = await ev(page, `return { ...(await mod("/src/tools/state.svelte.ts")).toolState.cropInfo }`);
    check("crop: Control disables snapping", info2.w !== 680 && Math.abs(info2.w - (680 + 3 / z)) <= 1, info2);
    await page.close();
  },

  async multiTransform() {
    const page = await setup();
    const { A, B } = await twoLayers(page);
    await ev(page, `(await mod("/src/pro/state.svelte.ts")).pro.selectedIds = a;`, [A, B]);
    await setTool(page, "transform");
    await settle(page, 600);
    const f = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("transform box spans both selected layers", f.layers === 2 && f.x === 300 && f.y === 200 && f.w === 380 && f.h === 280, f);
    // Type 50 into Scale and apply: both layers halve about the common centre (490, 340).
    const scale = page.getByTestId("transform-scale").locator("input");
    await scale.click();
    await scale.fill("50");
    await scale.press("Enter");
    await settle(page, 400);
    await page.screenshot({ path: `${OUT}/07-multi-transform-scale-50.png` });
    const u0 = await undoLen(page);
    await page.getByTestId("transform-apply").click();
    await settle(page, 700);
    const a = await bounds(page, A);
    const b = await bounds(page, B);
    const near = (r, x, y, w, h) => r && Math.abs(r.x - x) <= 1 && Math.abs(r.y - y) <= 1 && Math.abs(r.w - w) <= 2 && Math.abs(r.h - h) <= 2;
    check("scale 50% transforms both layers as one box", near(a, 395, 270, 50, 30) && near(b, 545, 370, 40, 40), { a, b });
    check("one history step", (await undoLen(page)) === u0 + 1, await lastUndo(page));
    // Move tool drags both selected layers together.
    await setTool(page, "move");
    await settle(page);
    const z = await zoomOf(page);
    const p = await at(page, 410, 280);
    await dragView(page, p, { x: p.x + 40 * z, y: p.y + 20 * z });
    const a2 = await bounds(page, A);
    const b2 = await bounds(page, B);
    check("move tool moves every selected layer", a2.x - a.x === b2.x - b.x && a2.y - a.y === b2.y - b.y && a2.x !== a.x, { a2, b2 });
    await page.close();
  },

  async groupTransform() {
    const page = await setup();
    const { A, B } = await twoLayers(page);
    const g = await ev(page, `const r = await editor.exec({ op: "layer.group", ids: a }); await editor.exec({ op: "layer.set-active", id: r.data.id }); return r.data.id;`, [A, B]);
    await ev(page, `(await mod("/src/pro/state.svelte.ts")).pro.selectedIds = [a];`, g);
    await setTool(page, "transform");
    await settle(page, 600);
    const f = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("a group's box is its children's bounds", f.x === 300 && f.y === 200 && f.w === 380 && f.h === 280, f);
    // ↑ in X steps 1, Shift+↑ 10: live.
    const x = page.getByTestId("transform-x").locator("input");
    await x.click();
    await x.press("ArrowUp");
    await x.press("Shift+ArrowUp");
    await settle(page, 300);
    const f2 = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("X field steps with arrows (1, Shift 10)", f2.x === 311, f2.x);
    const angle = page.getByTestId("transform-angle").locator("input");
    await angle.click();
    await angle.fill("90");
    await angle.press("Enter");
    await settle(page, 400);
    await page.screenshot({ path: `${OUT}/08-group-transform-fields.png` });
    await page.getByTestId("transform-cancel").click();
    await settle(page, 500);
    const f3 = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("Cancel restores the box", f3.x === 300 && f3.angle === 0, f3);
    // Now apply just the X nudge and check both children moved.
    const x2 = page.getByTestId("transform-x").locator("input");
    await x2.click();
    await x2.press("Shift+ArrowUp");
    await settle(page, 200);
    await page.getByTestId("transform-apply").click();
    await settle(page, 700);
    const a = await bounds(page, A);
    const b = await bounds(page, B);
    check("group transform moves every child", a?.x === 310 && b?.x === 610, { a, b });
    await page.close();
  },

  async numericFields() {
    const page = await setup();
    const { A } = await twoLayers(page);
    await setTool(page, "transform");
    await settle(page, 500);
    const w = page.getByTestId("transform-w").locator("input");
    await w.click();
    await w.fill("200");
    await w.press("Enter");
    await settle(page, 300);
    const f = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("W with the link on keeps proportions", f.w === 200 && f.h === 120 && f.scale === 200, f);
    await page.getByTestId("transform-link").click();
    await w.click();
    await w.fill("100");
    await w.press("Enter");
    await settle(page, 300);
    const f2 = await ev(page, `return { ...(await mod("/src/tools/transform-state.svelte.ts")).transformFields }`);
    check("W with the link off leaves H", f2.w === 100 && f2.h === 120, f2);
    const angle = page.getByTestId("transform-angle").locator("input");
    await angle.click();
    await angle.fill("90");
    await angle.press("Enter");
    await settle(page, 300);
    await page.screenshot({ path: `${OUT}/09-numeric-fields.png` });
    await page.getByTestId("transform-apply").click();
    await settle(page, 700);
    const b = await bounds(page, A);
    check("apply: 100×120 turned 90° gives a 120×100 layer", b && Math.abs(b.w - 120) <= 2 && Math.abs(b.h - 100) <= 2, b);
    await page.close();
  },

  async autoscroll() {
    const page = await setup({ w: 3000, h: 2000 });
    await ev(page, `editor.zoomAt(0.5)`);
    await setTool(page, "marquee-rect");
    await settle(page, 500);
    const cb = await page.getByTestId("canvas").boundingBox();
    const v0 = await ev(page, `return { ...editor.view }`);
    const start = { x: cb.x + cb.width / 2, y: cb.y + cb.height / 2 };
    const visibleRight = await ev(page, `return editor.toDoc(a, 0).x`, cb.width);
    await dragView(page, start, { x: cb.x + cb.width + 60, y: start.y + 40 }, {
      hold: async () => {
        await page.waitForTimeout(1200);
        await page.screenshot({ path: `${OUT}/10-autoscroll-marquee.png` });
      },
    });
    const v1 = await ev(page, `return { ...editor.view }`);
    const sel = await ev(page, `return editor.summary.selection?.bounds`);
    check("marquee past the edge pans the view", v1.cx - v0.cx > 100, { before: v0.cx, after: v1.cx });
    check("the marquee keeps extending past what was visible", sel && sel.x + sel.w > visibleRight + 100, { sel, visibleRight });
    // Moving a layer past the edge scrolls too, and the layer follows.
    await ev(page, `editor.view = { cx: 900, cy: 1000, zoom: 0.5 }`);
    await settle(page, 300);
    await ev(page, `await editor.exec({ op: "select.none" }); const r = await editor.exec({ op: "layer.import", width: 100, height: 100, x: a.x, y: a.y, name: "M" }, new Uint8Array(40000).fill(200)); editor.view = { ...editor.view };`, await ev(page, `const d = editor.toDoc(a[0], a[1]); return { x: Math.round(d.x), y: Math.round(d.y) }`, [cb.width / 2, cb.height / 2]));
    await setTool(page, "move");
    await settle(page, 400);
    const id = await ev(page, `return editor.summary.active`);
    const b0 = await bounds(page, id);
    const p0 = await at(page, b0.x + 50, b0.y + 50);
    const vv0 = await ev(page, `return editor.view.cx`);
    await dragView(page, p0, { x: cb.x + cb.width + 50, y: p0.y }, { hold: () => page.waitForTimeout(800) });
    const b1 = await bounds(page, id);
    const vv1 = await ev(page, `return editor.view.cx`);
    check("moving a layer past the edge pans and carries the layer", vv1 - vv0 > 50 && b1.x - b0.x > (cb.width / 2) / 0.5, { vv0, vv1, b0, b1 });
    await page.close();
  },

  async zoomTool() {
    const page = await setup();
    await setTool(page, "zoom");
    await settle(page);
    const z0 = await zoomOf(page);
    const a = await at(page, 400, 300);
    const b = await at(page, 600, 400);
    await dragView(page, a, b, { hold: () => page.screenshot({ path: `${OUT}/11-zoom-drag-rect.png` }) });
    const v = await ev(page, `return { ...editor.view, vw: editor.viewport.width, vh: editor.viewport.height }`);
    const want = Math.min(v.vw / 200, v.vh / 100);
    check("drag a rectangle zooms to fit it", Math.abs(v.zoom - want) < 0.02 && Math.abs(v.cx - 500) < 1 && Math.abs(v.cy - 350) < 1, { z0, v, want });
    await page.keyboard.down("Alt");
    const c = await page.getByTestId("canvas").boundingBox();
    await page.mouse.move(c.x + 300, c.y + 300);
    const cursor = await page.$eval(".ops-canvas__overlay", (e) => getComputedStyle(e).cursor);
    await page.mouse.down();
    await page.mouse.up();
    await page.keyboard.up("Alt");
    await settle(page);
    const z2 = await zoomOf(page);
    check("Alt shows the zoom-out cursor", cursor === "zoom-out", cursor);
    check("Alt-click zooms out", z2 < v.zoom, { before: v.zoom, after: z2 });
    await page.mouse.click(c.x + 300, c.y + 300);
    await settle(page);
    check("click zooms in", (await zoomOf(page)) > z2);
    await page.close();
  },

  async pixelGestures() {
    const page = await setup();
    // A red block on a transparent layer, selected.
    const id = await ev(page, `const r = await editor.exec({ op: "layer.import", width: 100, height: 60, x: 300, y: 200, name: "A" }, ${block(100, 60, [220, 30, 30])}); await editor.exec({ op: "select.rect", x: 300, y: 200, width: 100, height: 60 }); return r.data.id;`);
    await setTool(page, "move");
    await settle(page);
    const z = await zoomOf(page);
    const from = await at(page, 350, 230);
    const u0 = await undoLen(page);
    await page.keyboard.down("Alt");
    await dragView(page, from, { x: from.x + 150 * z, y: from.y }, { hold: () => page.screenshot({ path: `${OUT}/12-alt-drag-duplicate-pixels.png` }) });
    await page.keyboard.up("Alt");
    const src = await pixel(page, id, 320, 220);
    const copy = await pixel(page, id, 470, 220);
    const sel = await ev(page, `return editor.summary.selection?.bounds`);
    const layers = await ev(page, `return editor.summary.layers.length`);
    check("Alt-drag copies the selected pixels (source kept)", src[3] === 255 && src[0] === 220 && copy[3] === 255 && copy[0] === 220, { src, copy });
    check("the selection follows the copy; no new layer", sel?.x === 450 && layers === 2, { sel, layers });
    check("Alt-drag duplicate is one undo step", (await undoLen(page)) === u0 + 1 && (await lastUndo(page)) === "Duplicate Pixels", await ev(page, `return editor.summary.history.undo.slice(-2)`));
    // Cmd/Ctrl+arrow nudges the selected pixels, Shift ×10, from any tool.
    await setTool(page, "marquee-rect");
    await settle(page);
    await page.mouse.move(from.x, from.y);
    await page.keyboard.press(`${MOD}+ArrowRight`);
    await settle(page, 400);
    const sel2 = await ev(page, `return editor.summary.selection?.bounds`);
    const left = await pixel(page, id, 450, 220);
    const right = await pixel(page, id, 550, 220);
    check("Mod+→ moves the selected pixels 1 px", sel2?.x === 451 && left[3] === 0 && right[3] === 255, { sel2, left, right });
    await page.keyboard.press(`${MOD}+Shift+ArrowDown`);
    await settle(page, 400);
    const sel3 = await ev(page, `return editor.summary.selection?.bounds`);
    check("Mod+Shift+↓ moves 10 px", sel3?.y === 210 && (await pixel(page, id, 460, 265))[3] === 255, sel3);
    await page.screenshot({ path: `${OUT}/13-mod-arrow-nudge.png` });
    await page.close();
  },

  async lassoAlt() {
    const page = await setup();
    await setTool(page, "lasso");
    await settle(page);
    const P = async (x, y) => at(page, x, y);
    const p0 = await P(200, 200);
    await page.mouse.move(p0.x, p0.y);
    await page.mouse.down();
    for (let x = 200; x <= 600; x += 20) {
      const q = await P(x, 200 + (x % 40 ? 3 : -3));
      await page.mouse.move(q.x, q.y);
    }
    await page.keyboard.down("Alt");
    // Wander far off the line while Alt is down: only the end point counts.
    for (const [x, y] of [[700, 120], [780, 300], [700, 450], [600, 500]]) {
      const q = await P(x, y);
      await page.mouse.move(q.x, q.y, { steps: 4 });
    }
    await page.screenshot({ path: `${OUT}/14-lasso-alt-straight.png` });
    await page.keyboard.up("Alt");
    for (let x = 600; x >= 200; x -= 20) {
      const q = await P(x, 500 + (x % 40 ? 3 : -3));
      await page.mouse.move(q.x, q.y);
    }
    await page.mouse.up();
    await settle(page, 500);
    const b = await ev(page, `return editor.summary.selection?.bounds`);
    check("Alt lays a straight segment (the wander is not selected)", b && b.x + b.w <= 606 && (await selAt(page, 700, 300)) === 0 && (await selAt(page, 400, 350)) === 255, b);
    // Release the button with Alt held: clicks add corners, letting go of Alt closes.
    await ev(page, `await editor.exec({ op: "select.none" })`);
    const q0 = await P(100, 100);
    const q1 = await P(300, 100);
    await page.mouse.move(q0.x, q0.y);
    await page.mouse.down();
    await page.keyboard.down("Alt");
    await page.mouse.move(q1.x, q1.y, { steps: 6 });
    await page.mouse.up();
    const q2 = await P(300, 300);
    await page.mouse.click(q2.x, q2.y);
    const q3 = await P(100, 300);
    await page.mouse.move(q3.x, q3.y, { steps: 4 });
    await page.screenshot({ path: `${OUT}/15-lasso-alt-open.png` });
    await page.mouse.click(q3.x, q3.y);
    await page.keyboard.up("Alt");
    await settle(page, 500);
    const b2 = await ev(page, `return editor.summary.selection?.bounds`);
    check("Alt-click corners, Alt released closes", b2 && Math.abs(b2.x - 100) <= 2 && Math.abs(b2.w - 200) <= 3 && Math.abs(b2.h - 200) <= 3, b2);
    await page.close();
  },
};

for (const [name, fn] of Object.entries(tests)) {
  if (only && name !== only) continue;
  try {
    await fn();
  } catch (e) {
    check(name, false, e.message);
  }
}
await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length ? 1 : 0);
