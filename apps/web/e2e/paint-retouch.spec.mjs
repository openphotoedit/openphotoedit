// Painting and retouching flows (spot-heal stroke, liquify, Shift axis lock,
// Alt eyedropper, clone preview, smoothed strokes), driven with real pointer
// events against the tools harness.
//   cd apps/web && npx vite --port 5243 --strictPort
//   URL=http://localhost:5243/src/tools/dev/harness.html node apps/web/e2e/paint-retouch.spec.mjs [only]
// Screenshots land in target/tools/out/paint-*.png.
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

async function setup() {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 }, deviceScaleFactor: 1 });
  const logs = [];
  page.on("console", (m) => {
    if (m.type() === "error") logs.push(`${m.type()}: ${m.text()}`);
  });
  page.on("pageerror", (e) => logs.push(`pageerror: ${e.message}`));
  await page.goto(URL_);
  await page.waitForFunction(() => window.__ops?.editor.ready);
  await page.getByTestId("open").setInputFiles(PHOTO);
  await page.waitForFunction(() => window.__ops.editor.hasDocument && !window.__ops.editor.busy);
  await page.waitForTimeout(600);
  return { page, logs };
}

/** A fresh white document instead of the photo. */
async function blank(page, w = 800, h = 600) {
  await page.evaluate(
    async ([w, h]) => {
      const ed = window.__ops.editor;
      await ed.exec({ op: "doc.new", width: w, height: h, background: { r: 255, g: 255, b: 255, a: 255 } });
    },
    [w, h],
  );
  await page.waitForTimeout(500);
  // Fit the new document in view.
  await page.evaluate(() => window.__ops.editor.fit());
  await page.waitForTimeout(300);
}

const summary = (page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ops.editor.summary)));
const setTool = (page, id) => page.evaluate((id) => (window.__ops.editor.tool = id), id);
const undoLen = async (page) => (await summary(page)).history.undo.length;

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

/** Drag through document points (a polyline), `steps` moves per leg. */
async function drag(page, pts, { steps = 12, before, hold } = {}) {
  const v = [];
  for (const p of pts) v.push(await at(page, p[0], p[1]));
  await page.mouse.move(v[0].x, v[0].y);
  if (before) await before();
  await page.mouse.down();
  for (let k = 1; k < v.length; k++) {
    for (let i = 1; i <= steps; i++) {
      await page.mouse.move(v[k - 1].x + ((v[k].x - v[k - 1].x) * i) / steps, v[k - 1].y + ((v[k].y - v[k - 1].y) * i) / steps);
      await page.waitForTimeout(8);
    }
    if (k === 1 && hold) await hold();
  }
  await page.mouse.up();
  await page.waitForTimeout(400);
}

const region = (page, x, y, w, h) => page.evaluate(async ([x, y, w, h]) => Array.from(await window.__ops.editor.engine.call("region", x, y, w, h)), [x, y, w, h]);
const idle = (page) => page.waitForFunction(() => !window.__ops.editor.busy).then(() => page.waitForTimeout(300));

function check(name, ok, detail) {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
}

const tests = {
  async spotHeal() {
    const { page, logs } = await setup();
    await setTool(page, "spot-heal");
    await page.evaluate(() => (window.__ops.editor.primary = window.__ops.editor.primary));
    const n0 = await undoLen(page);
    // Paint a long scratch across the face.
    await drag(page, [[1150, 420], [1300, 460], [1450, 440]], {
      steps: 20,
      hold: () => page.screenshot({ path: `${OUT}/paint-spot-heal-drag.png` }),
    });
    await idle(page);
    const s = await summary(page);
    check("one spot-heal stroke is one history step", s.history.undo.length === n0 + 1, `${s.history.undo.length - n0} steps: ${s.history.undo.slice(-2).join(", ")}`);
    check("the step is labelled Spot Healing Brush", s.history.undo.at(-1) === "Spot Healing Brush", s.history.undo.at(-1));
    await page.screenshot({ path: `${OUT}/paint-spot-heal-after.png` });
    // A click still heals one spot.
    const c = await at(page, 1250, 700);
    await page.mouse.click(c.x, c.y);
    await idle(page);
    check("a click heals one spot", (await undoLen(page)) === n0 + 2);
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async liquify() {
    const { page, logs } = await setup();
    const registered = await page.evaluate(() => !!window.__ops.editor && (window.__ops.editor.tool = "liquify") && true);
    await setTool(page, "liquify");
    await page.waitForTimeout(300);
    check("liquify tool is selectable", registered && (await page.evaluate(() => window.__ops.editor.tool)) === "liquify");
    const before = await region(page, 1000, 300, 600, 400);
    const n0 = await undoLen(page);
    await drag(page, [[1150, 500], [1450, 500]], {
      steps: 25,
      hold: () => page.screenshot({ path: `${OUT}/paint-liquify-drag.png` }),
    });
    await idle(page);
    const after = await region(page, 1000, 300, 600, 400);
    let changed = 0;
    for (let i = 0; i < before.length; i += 4) if (Math.abs(before[i] - after[i]) > 8) changed++;
    const s = await summary(page);
    check("a liquify drag moves pixels", changed > 2000, `${changed} pixels changed`);
    check("one liquify stroke is one history step", s.history.undo.length === n0 + 1 && s.history.undo.at(-1) === "Liquify", s.history.undo.slice(-2).join(", "));
    await page.screenshot({ path: `${OUT}/paint-liquify-after.png` });
    // Mode picker: twirl.
    await page.getByTestId("liquify-mode").selectOption("twirl-cw").catch(() => null);
    const c = await at(page, 1300, 900);
    await page.mouse.move(c.x, c.y);
    await page.mouse.down();
    for (let i = 0; i < 20; i++) {
      await page.mouse.move(c.x + (i % 2), c.y);
      await page.waitForTimeout(16);
    }
    await page.mouse.up();
    await idle(page);
    check("twirl is another single step", (await undoLen(page)) === n0 + 2);
    await page.screenshot({ path: `${OUT}/paint-liquify-twirl.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async axisLock() {
    const { page, logs } = await setup();
    await blank(page);
    await setTool(page, "brush");
    await page.evaluate(() => {
      window.__ops.editor.primary = { r: 230, g: 20, b: 20, a: 255 };
    });
    await page.evaluate(() => {
      const s = JSON.parse(localStorage.getItem("ops.toolSettings") ?? "{}");
      return s;
    });
    // Press, move a little, then hold Shift and go diagonally.
    await drag(page, [[150, 300], [180, 305], [650, 520]], {
      steps: 16,
      hold: () => page.keyboard.down("Shift"),
    });
    await page.keyboard.up("Shift");
    await idle(page);
    const px = await region(page, 0, 0, 800, 600);
    const size = await page.evaluate(() => JSON.parse(localStorage.getItem("ops.toolSettings") ?? "{}").brushSize ?? 30);
    let minY = 1e9;
    let maxY = -1;
    let maxX = -1;
    for (let y = 0; y < 600; y++)
      for (let x = 250; x < 800; x++) {
        const i = (y * 800 + x) * 4;
        if (px[i + 1] < 200) {
          minY = Math.min(minY, y);
          maxY = Math.max(maxY, y);
          maxX = Math.max(maxX, x);
        }
      }
    const anchorY = 305;
    const ok = maxX > 600 && minY >= anchorY - size / 2 - 1 && maxY <= anchorY + size / 2 + 1;
    check("Shift mid-drag locks the stroke horizontally (no off-axis end)", ok, `painted x→${maxX}, rows ${minY}..${maxY}, anchor ${anchorY} ± ${size / 2 + 1}`);
    await page.screenshot({ path: `${OUT}/paint-axis-lock.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async altEyedropper() {
    const { page, logs } = await setup();
    await blank(page);
    await page.evaluate(async () => {
      const ed = window.__ops.editor;
      await ed.exec({ op: "select.rect", x: 400, y: 0, width: 400, height: 600 });
      await ed.exec({ op: "layer.fill-selection", color: { r: 20, g: 120, b: 230, a: 255 } });
      await ed.exec({ op: "select.none" });
      ed.primary = { r: 0, g: 0, b: 0, a: 255 };
    });
    await idle(page);
    for (const tool of ["brush", "gradient"]) {
      await setTool(page, tool);
      await page.evaluate(() => (window.__ops.editor.primary = { r: 0, g: 0, b: 0, a: 255 }));
      const n0 = await undoLen(page);
      const c = await at(page, 600, 300);
      await page.mouse.move(c.x, c.y);
      await page.keyboard.down("Alt");
      await page.mouse.move(c.x + 1, c.y, { steps: 2 });
      const cursor = await page.evaluate(() => window.__ops.editor.cursor ?? "");
      await page.mouse.down();
      await page.waitForTimeout(150);
      if (tool === "brush") await page.screenshot({ path: `${OUT}/paint-alt-eyedropper.png` });
      await page.mouse.up();
      await page.keyboard.up("Alt");
      await idle(page);
      const prim = await page.evaluate(() => ({ ...window.__ops.editor.primary }));
      check(`${tool}: Alt-click samples the colour`, prim.r === 20 && prim.g === 120 && prim.b === 230, JSON.stringify(prim));
      check(`${tool}: Alt-click paints nothing`, (await undoLen(page)) === n0);
      check(`${tool}: the cursor is a pipette while Alt is held`, /svg/.test(cursor), cursor.slice(0, 40));
      await page.mouse.move(c.x + 5, c.y, { steps: 2 });
      const after = await page.evaluate(() => window.__ops.editor.cursor);
      check(`${tool}: releasing Alt returns to the tool`, !after, String(after));
    }
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async clonePreview() {
    const { page, logs } = await setup();
    await blank(page);
    await page.evaluate(async () => {
      const ed = window.__ops.editor;
      await ed.exec({ op: "select.rect", x: 0, y: 0, width: 400, height: 600 });
      await ed.exec({ op: "layer.fill-selection", color: { r: 220, g: 30, b: 30, a: 255 } });
      await ed.exec({ op: "select.rect", x: 400, y: 0, width: 400, height: 600 });
      await ed.exec({ op: "layer.fill-selection", color: { r: 30, g: 60, b: 220, a: 255 } });
      await ed.exec({ op: "select.none" });
    });
    await idle(page);
    await setTool(page, "clone");
    const src = await at(page, 200, 300);
    await page.keyboard.down("Alt");
    await page.mouse.click(src.x, src.y);
    await page.keyboard.up("Alt");
    const dst = await at(page, 600, 300);
    await page.mouse.move(dst.x - 4, dst.y);
    await page.mouse.move(dst.x, dst.y, { steps: 3 });
    await page.waitForTimeout(500);
    await page.mouse.move(dst.x + 1, dst.y);
    await page.waitForTimeout(300);
    const centre = await page.evaluate(([x, y]) => {
      const c = document.querySelector(".ops-canvas__overlay");
      const r = c.getBoundingClientRect();
      const k = c.width / r.width;
      return Array.from(c.getContext("2d").getImageData(Math.round((x - r.left) * k), Math.round((y - r.top) * k), 1, 1).data);
    }, [dst.x + 4, dst.y + 4]);
    await page.screenshot({ path: `${OUT}/paint-clone-preview.png` });
    check("the clone preview shows the source (red) inside the brush over blue", centre[0] > 120 && centre[2] < 120 && centre[3] > 100, JSON.stringify(centre));
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },

  async smoothStroke() {
    const { page, logs } = await setup();
    await blank(page);
    await setTool(page, "brush");
    // Four pointer samples 60° apart on a circle: the stroke should be round.
    const pts = [];
    for (let i = 0; i <= 3; i++) pts.push([400 + 200 * Math.cos((i * Math.PI) / 3), 450 - 200 * Math.sin((i * Math.PI) / 3)]);
    await drag(page, pts, { steps: 1 });
    await idle(page);
    const px = await region(page, 0, 0, 800, 600);
    // Midway between samples a chord would pass 26.8 px inside the circle,
    // beyond a 30-px brush; Catmull–Rom stays within about 5 px.
    let hits = 0;
    for (const deg of [30, 90, 150]) {
      const x = Math.round(400 + 200 * Math.cos((deg * Math.PI) / 180));
      const y = Math.round(450 - 200 * Math.sin((deg * Math.PI) / 180));
      const i = (y * 800 + x) * 4;
      if (px[i] < 128) hits++;
    }
    check("coarse samples paint a round arc", hits === 3, `${hits}/3 mid-arc points painted`);
    await page.screenshot({ path: `${OUT}/paint-smooth-stroke.png` });
    if (logs.length) console.log(logs.join("\n"));
    await page.close();
  },
};

for (const [name, fn] of Object.entries(tests)) {
  if (only && name !== only) continue;
  try {
    await fn();
  } catch (e) {
    check(name, false, String(e).split("\n")[0]);
  }
}
await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length ? 1 : 0);
