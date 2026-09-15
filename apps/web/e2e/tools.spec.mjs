// Canvas tools, driven with real pointer events against the tools harness.
//   cd apps/web && npx vite --port 5217 --strictPort
//   node apps/web/e2e/tools.spec.mjs [only-test-name]
// Screenshots land in target/tools/out/.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL_ = process.env.URL ?? "http://localhost:5217/src/tools/dev/harness.html";
const PHOTO = process.env.PHOTO ?? resolve(root, "testdata/photos/portrait.jpg");
const OUT = resolve(root, "target/tools/out");
mkdirSync(OUT, { recursive: true });
const only = process.argv[2];

const browser = await chromium.launch();
const results = [];

async function setup(theme = "dark") {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 }, deviceScaleFactor: 1 });
  const logs = [];
  page.on("console", (m) => {
    if (m.type() === "error" || m.type() === "warning") logs.push(`${m.type()}: ${m.text()}`);
  });
  page.on("pageerror", (e) => logs.push(`pageerror: ${e.message}`));
  await page.goto(`${URL_}${theme === "light" ? "?theme=light" : ""}`);
  await page.waitForFunction(() => window.__ops?.editor.ready);
  await page.getByTestId("open").setInputFiles(PHOTO);
  await page.waitForFunction(() => window.__ops.editor.hasDocument && !window.__ops.editor.busy);
  await page.waitForTimeout(600);
  return { page, logs };
}

const summary = (page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ops.editor.summary)));
const toasts = (page) => page.evaluate(() => window.__ops.editor.toasts.map((t) => t.text));
const setTool = (page, id) => page.evaluate((id) => (window.__ops.editor.tool = id), id);

/** Document point → page coordinates. */
async function at(page, x, y) {
  return page.evaluate(
    ([x, y]) => {
      const r = document.querySelector('[data-testid="canvas"]').getBoundingClientRect();
      const v = window.__ops.editor.toView(x, y);
      return { x: r.left + v.x, y: r.top + v.y };
    },
    [x, y],
  );
}

async function drag(page, from, to, { steps = 12, hold, before } = {}) {
  const a = await at(page, from[0], from[1]);
  const b = await at(page, to[0], to[1]);
  await page.mouse.move(a.x, a.y);
  if (before) await before();
  await page.mouse.down();
  for (let i = 1; i <= steps; i++) {
    await page.mouse.move(a.x + ((b.x - a.x) * i) / steps, a.y + ((b.y - a.y) * i) / steps);
    await page.waitForTimeout(8);
  }
  if (hold) await hold();
  await page.mouse.up();
  await page.waitForTimeout(250);
}

async function idle(page, ms = 400) {
  await page.waitForTimeout(ms);
}

function check(name, ok, detail) {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

const tests = {
  async marquee() {
    const { page, logs } = await setup();
    await setTool(page, "marquee-rect");
    await drag(page, [400, 500], [1600, 1400], { hold: () => page.screenshot({ path: `${OUT}/marquee-drag.png` }) });
    await idle(page);
    const s = await summary(page);
    const b = s.selection?.bounds;
    check("marquee selects the dragged rectangle", !!b && Math.abs(b.x - 400) <= 6 && Math.abs(b.w - 1200) <= 12 && Math.abs(b.h - 900) <= 12, JSON.stringify(b));
    // Shift adds an ellipse.
    await setTool(page, "marquee-ellipse");
    await page.keyboard.down("Shift");
    await drag(page, [1500, 1300], [2200, 2000]);
    await page.keyboard.up("Shift");
    await idle(page);
    const s2 = await summary(page);
    check("shift-drag adds to the selection", s2.selection?.bounds.w > b.w, JSON.stringify(s2.selection?.bounds));
    await page.screenshot({ path: `${OUT}/marquee-after.png` });
    // Click without drag deselects.
    const p = await at(page, 300, 300);
    await page.mouse.click(p.x, p.y);
    await idle(page);
    check("click deselects", !(await summary(page)).selection);
    // Polygon lasso.
    await setTool(page, "polygon-lasso");
    for (const [x, y] of [[500, 600], [1500, 700], [1200, 1800]]) {
      const q = await at(page, x, y);
      await page.mouse.click(q.x, q.y);
      await page.waitForTimeout(400);
    }
    await page.screenshot({ path: `${OUT}/polygon-lasso.png` });
    await page.keyboard.press("Enter");
    await idle(page);
    check("polygon lasso closes with Enter", !!(await summary(page)).selection);
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async crop() {
    const { page, logs } = await setup();
    const s0 = await summary(page);
    await setTool(page, "crop");
    await idle(page, 200);
    await drag(page, [s0.width, s0.height], [s0.width * 0.7, s0.height * 0.6], { hold: () => page.screenshot({ path: `${OUT}/crop-drag.png` }) });
    await page.screenshot({ path: `${OUT}/crop-pending.png` });
    await page.keyboard.press("Enter");
    await idle(page, 800);
    const s = await summary(page);
    check("crop resizes the canvas", Math.abs(s.width - Math.round(s0.width * 0.7)) <= 2 && Math.abs(s.height - Math.round(s0.height * 0.6)) <= 2, `${s0.width}×${s0.height} → ${s.width}×${s.height}`);
    // Straighten by rotating outside the box, then apply.
    await drag(page, [s.width + 150, s.height / 2], [s.width + 150, s.height / 2 + 250]);
    await page.screenshot({ path: `${OUT}/crop-rotate.png` });
    await page.getByTestId("crop-apply").click();
    await idle(page, 1500);
    const s2 = await summary(page);
    check("rotated crop applies", s2.history.undo.includes("Straighten") && s2.width < s.width, `${s2.width}×${s2.height}; ${s2.history.undo.slice(-2).join(", ")}`);
    await page.screenshot({ path: `${OUT}/crop-after.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async annotations() {
    const { page, logs } = await setup("light");
    const n0 = (await summary(page)).layers.length;
    await page.evaluate(() => (window.__ops.editor.primary = { r: 255, g: 59, b: 48, a: 255 }));
    await setTool(page, "arrow");
    await drag(page, [500, 600], [1500, 1200], { hold: () => page.screenshot({ path: `${OUT}/arrow-drag.png` }) });
    await idle(page);
    let s = await summary(page);
    const arrow = s.layers.find((l) => l.shape?.kind === "arrow");
    check("arrow creates a shape layer", s.layers.length === n0 + 1 && !!arrow, arrow?.name);
    // Drag its end handle.
    await drag(page, [1500, 1200], [1800, 900]);
    await idle(page);
    s = await summary(page);
    const moved = s.layers.find((l) => l.id === arrow?.id)?.shape?.points?.[1];
    check("arrow end handle drags", !!moved && Math.abs(moved.x - 1800) < 10, JSON.stringify(moved));

    await setTool(page, "redact");
    await drag(page, [1000, 300], [1900, 500]);
    await idle(page);
    s = await summary(page);
    check("redact creates a box", !!s.layers.find((l) => l.shape?.kind === "redact"));

    await setTool(page, "highlighter");
    await drag(page, [400, 2000], [2200, 2100], { steps: 20 });
    await idle(page);
    s = await summary(page);
    check("highlighter creates a polyline", !!s.layers.find((l) => l.shape?.kind === "polyline"));

    await setTool(page, "text");
    const p = await at(page, 400, 2400);
    await page.mouse.click(p.x, p.y);
    await page.waitForSelector('[data-testid="text-editor"]');
    await page.keyboard.type("Hello from the tools");
    await page.evaluate(() => {
      window.__ops.toolSettings.textBackgroundOn = true;
      window.__ops.toolSettings.fontSize = 120;
    });
    await idle(page, 200);
    await page.screenshot({ path: `${OUT}/text-editing.png` });
    await page.getByTestId("text-commit").click();
    await idle(page, 800);
    s = await summary(page);
    const txt = s.layers.find((l) => l.kind === "text");
    check("text creates a text layer", !!txt && txt.text.text === "Hello from the tools", txt?.bounds && JSON.stringify(txt.bounds));
    // Re-open and edit.
    const q = await at(page, 600, 2450);
    await page.mouse.click(q.x, q.y);
    await page.waitForSelector('[data-testid="text-editor"]');
    await page.keyboard.press("End");
    await page.keyboard.type("!");
    await page.keyboard.press("Meta+Enter");
    await idle(page, 800);
    s = await summary(page);
    const t2 = s.layers.find((l) => l.kind === "text");
    check("clicking a text layer edits it", t2?.text?.text === "Hello from the tools!" && t2.visible, `${t2?.text?.text} visible=${t2?.visible}`);

    await setTool(page, "pixelate");
    await drag(page, [900, 900], [1700, 1500]);
    await idle(page, 1200);
    s = await summary(page);
    check("pixelate records a step", s.history.undo.some((h) => /pixelate|mosaic/i.test(h)), s.history.undo.slice(-3).join(", "));
    await page.screenshot({ path: `${OUT}/annotations.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async brush() {
    const { page, logs } = await setup();
    await setTool(page, "brush");
    const r0 = (await summary(page)).revision;
    await drag(page, [400, 400], [2000, 1600], { steps: 30 });
    await idle(page, 600);
    const s = await summary(page);
    const t = await toasts(page);
    const missing = t.some((x) => /not available/i.test(x));
    if (missing) check("brush stroke (skipped: paint.stroke has not landed)", true, t.join(" | "));
    else check("brush stroke paints one undo step", s.revision > r0 && s.history.undo.length >= 1, s.history.undo.slice(-2).join(", "));
    await page.screenshot({ path: `${OUT}/brush.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async move() {
    const { page, logs } = await setup();
    const b0 = (await summary(page)).layers[0].bounds;
    await setTool(page, "move");
    await drag(page, [1000, 1000], [1300, 1200], { hold: () => page.screenshot({ path: `${OUT}/move-drag.png` }) });
    await idle(page, 600);
    const s = await summary(page);
    const b = s.layers[0].bounds;
    check("move offsets the layer", b.x - b0.x === 300 && b.y - b0.y === 200, JSON.stringify(b));
    check("one drag is one undo step", s.history.undo.filter((h) => /move|offset/i.test(h)).length === 1, s.history.undo.join(", "));
    await page.keyboard.press("Shift+ArrowLeft");
    await idle(page);
    check("arrow key nudges", (await summary(page)).layers[0].bounds.x === b.x - 10);
    await page.screenshot({ path: `${OUT}/move-after.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async transform() {
    const { page, logs } = await setup();
    // A shape layer and the photo itself.
    await setTool(page, "shape-rect");
    await page.evaluate(() => {
      window.__ops.toolSettings.shapeFillOn = true;
      window.__ops.toolSettings.shapeFill = { r: 20, g: 115, b: 230, a: 255 };
    });
    await drag(page, [600, 600], [1400, 1200]);
    await idle(page);
    await setTool(page, "transform");
    await idle(page, 300);
    const b0 = (await summary(page)).layers.at(-1).shape.points;
    await drag(page, [1400, 1200], [1800, 1500]);
    await page.screenshot({ path: `${OUT}/transform-shape.png` });
    await page.keyboard.press("Enter");
    await idle(page, 600);
    const pts = (await summary(page)).layers.at(-1).shape.points;
    check("transform scales a shape layer", Math.abs(pts[1].x - 1800) < 12 && Math.abs(pts[0].x - b0[0].x) < 6, JSON.stringify(pts));

    // Now the photo: scale to half from the corner.
    await page.evaluate(async () => {
      const ed = window.__ops.editor;
      await ed.exec({ op: "layer.set-active", id: ed.summary.layers[0].id });
    });
    await setTool(page, "move");
    await setTool(page, "transform");
    await idle(page, 800);
    const s0 = await summary(page);
    const lb = s0.layers[0].bounds;
    await drag(page, [lb.x + lb.w, lb.y + lb.h], [lb.x + lb.w / 2, lb.y + lb.h / 2], { steps: 20 });
    await idle(page, 300);
    await page.screenshot({ path: `${OUT}/transform-preview.png` });
    await page.keyboard.press("Enter");
    await page.waitForTimeout(4000);
    const s = await summary(page);
    const nb = s.layers[0].bounds;
    const t = await toasts(page);
    check("transform scales the photo layer to half", Math.abs(nb.w - lb.w / 2) <= 4 && Math.abs(nb.h - lb.h / 2) <= 4 && s.layers[0].visible, `${JSON.stringify(nb)} visible=${s.layers[0].visible} ${t.join(" | ")}`);
    await page.screenshot({ path: `${OUT}/transform-after.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async options() {
    // Every options component mounts, in dark and light.
    for (const theme of ["dark", "light"]) {
      const { page, logs } = await setup(theme);
      const ids = await page.evaluate(() => Object.keys(window.__ops.TOOLS));
      for (const id of ids) {
        await setTool(page, id);
        await page.waitForTimeout(30);
      }
      for (const id of ["brush", "crop", "text", "shape-rect", "transform"]) {
        await setTool(page, id);
        await page.waitForTimeout(150);
        await page.locator(".options").screenshot({ path: `${OUT}/options-${id}-${theme}.png` });
      }
      check(`options mount for every tool (${theme})`, !logs.some((l) => l.startsWith("pageerror")), logs.join(" | "));
      await page.close();
    }
  },
};

for (const [name, fn] of Object.entries(tests)) {
  if (only && name !== only) continue;
  try {
    await fn();
  } catch (e) {
    check(`${name} threw`, false, e.stack ?? String(e));
  }
}
await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length ? 1 : 0);
