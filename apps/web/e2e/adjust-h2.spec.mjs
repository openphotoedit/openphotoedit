// Adjustment depth (workstream H2): Hue/Saturation range bands, band handles,
// eyedroppers and the targeted-adjust hand; Levels black/gray/white droppers;
// the Grain adjustment; Add Noise seeds.
//
//   cd apps/web && npx vite --port 5242 --strictPort
//   URL=http://localhost:5242/ [IMG=<600×400 png>] node apps/web/e2e/adjust-h2.spec.mjs   (IMG is drawn when omitted)
//
// The test image: x<200 red (220,30,30), 200..400 blue (30,40,220),
// x≥400 warm grey (170,140,105) on top and neutral grey (128) below.
// Screenshots land in target/h2/out/.
import { chromium } from "playwright";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const URL = process.env.URL ?? "http://localhost:5242/";
let IMG = process.env.IMG;
const OUT = resolve(root, "target/h2/out");
mkdirSync(OUT, { recursive: true });

let failed = false;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failed = true;
}

const browser = await chromium.launch();
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })).newPage();

// With no IMG given, draw the documented test image in the page so the suite
// runs anywhere: x<200 red, 200..400 blue, x≥400 warm grey above / neutral
// grey below.
if (!IMG) {
  const bytes = await page.evaluate(async () => {
    const c = new OffscreenCanvas(600, 400);
    const g = c.getContext("2d");
    g.fillStyle = "rgb(220,30,30)"; g.fillRect(0, 0, 200, 400);
    g.fillStyle = "rgb(30,40,220)"; g.fillRect(200, 0, 200, 400);
    g.fillStyle = "rgb(170,140,105)"; g.fillRect(400, 0, 200, 200);
    g.fillStyle = "rgb(128,128,128)"; g.fillRect(400, 200, 200, 200);
    const b = await c.convertToBlob({ type: "image/png" });
    return Array.from(new Uint8Array(await b.arrayBuffer()));
  });
  IMG = resolve(OUT, "adjust-fixture.png");
  writeFileSync(IMG, Buffer.from(bytes));
}
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => m.type() === "error" && errors.push(`console: ${m.text()}`));
const MOD = process.platform === "darwin" ? "Meta" : "Control";
const settle = (ms = 500) => page.waitForTimeout(ms);
const shot = (name, el) => (el ?? page).screenshot({ path: `${OUT}/${name}.png` });

async function menu(top, ...path) {
  await page.getByTestId(`menu-${top}`).click();
  await page.getByTestId(`menu-${top}-popup`).waitFor();
  for (let i = 0; i < path.length - 1; i++) {
    await item(path[i]).hover();
    await settle(250);
  }
  await item(path[path.length - 1]).click();
}
function item(label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  return page.locator(".ops-menu .item").filter({ has: page.locator(".label", { hasText: exact }) }).last();
}

// Where the document sits on the base canvas: found from the red strip's left
// edge and the image's right edge along the middle row.
let map = null;
async function locate() {
  map = await page.evaluate(() => {
    const c = document.querySelector(".ops-canvas__base");
    const r = c.getBoundingClientRect();
    const g = c.getContext("2d");
    const y = Math.floor(c.height / 2);
    const row = g.getImageData(0, y, c.width, 1).data;
    let left = -1;
    let right = -1;
    for (let x = 0; x < c.width; x++) {
      const [R, G, B] = [row[x * 4], row[x * 4 + 1], row[x * 4 + 2]];
      if (left < 0 && R > 180 && G < 90 && B < 90) left = x;
    }
    for (let x = c.width - 1; x >= 0; x--) {
      const [R, G, B] = [row[x * 4], row[x * 4 + 1], row[x * 4 + 2]];
      if (Math.abs(R - G) < 40 && R > 90 && R < 200 && Math.abs(G - B) < 45) {
        right = x;
        break;
      }
    }
    const k = (right - left + 1) / 600;
    return { left: r.left + left, top: r.top + y - 200 * k, k, cw: r.width / c.width };
  });
}
const client = (x, y) => ({ x: map.left + x * map.k, y: map.top + y * map.k });
async function pixel(x, y) {
  const p = client(x, y);
  return page.evaluate(({ px, py }) => {
    const c = document.querySelector(".ops-canvas__base");
    const r = c.getBoundingClientRect();
    const d = c.getContext("2d").getImageData(Math.round(px - r.left), Math.round(py - r.top), 1, 1).data;
    return [d[0], d[1], d[2]];
  }, { px: p.x, py: p.y });
}
async function clickDoc(x, y) {
  const p = client(x, y);
  await page.mouse.click(p.x, p.y);
}
const near = (a, b, tol) => a.every((v, i) => Math.abs(v - b[i]) <= tol);
const readout = () => page.getByTestId("hue-band-readout").innerText();
const sliderValue = (testid) => page.getByTestId(testid).locator("input").first().inputValue();

try {
  await page.goto(URL);
  await page.evaluate(() => localStorage.clear());
  await page.goto(URL);
  await page.getByTestId("choose-pro").click();
  await page.getByTestId("pro-app").waitFor();
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu("file", "Open…")]);
  await chooser.setFiles(IMG);
  await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]') && !document.querySelector('[data-testid="empty-state"]'));
  await settle(1200);
  await locate();
  check("Image located on the canvas", map.k > 0.3, JSON.stringify(map));
  check("Red strip reads red", near(await pixel(100, 200), [220, 30, 30], 3), String(await pixel(100, 200)));

  // ---- Add Noise: two applications use different seeds
  const strip = async () => {
    const out = [];
    for (let i = 0; i < 12; i++) out.push((await pixel(420 + i * 12, 300))[0]);
    return out;
  };
  await menu("filter", "Noise", "Add Noise…");
  await page.getByTestId("dialog-ok").waitFor();
  await settle(700);
  await shot("05-add-noise-dialog");
  await page.getByTestId("dialog-ok").click();
  await settle(800);
  const n1 = await strip();
  await page.keyboard.press(`${MOD}+z`);
  await settle(500);
  await menu("filter", "Noise", "Add Noise…");
  await page.getByTestId("dialog-ok").waitFor();
  await settle(700);
  await page.getByTestId("dialog-ok").click();
  await settle(800);
  const n2 = await strip();
  check("Add Noise changes the grey patch", n1.some((v) => v !== 128), JSON.stringify(n1));
  check("A second Add Noise uses a new seed", JSON.stringify(n1) !== JSON.stringify(n2), `${n1} vs ${n2}`);
  await page.keyboard.press(`${MOD}+z`);
  await settle(500);

  // ---- Destructive Image › Adjustments › Levels: droppers sample the image
  // without the preview, and Cancel leaves it untouched.
  await menu("image", "Adjustments", "Levels…");
  await page.getByTestId("adjustment-dialog").waitFor();
  await settle(500);
  await page.getByTestId("levels-dropper-gray").click();
  // The dialog covers the right of the image; sample the red strip.
  await clickDoc(100, 300);
  await settle(1200);
  const dlg = await pixel(100, 300);
  check("Levels dialog gray dropper neutralises (preview)", Math.abs(dlg[0] - dlg[1]) <= 4 && Math.abs(dlg[1] - dlg[2]) <= 4, String(dlg));
  await shot("22-levels-dialog");
  await page.keyboard.press("Escape");
  await page.getByTestId("adjustment-dialog").getByRole("button", { name: "Cancel" }).click();
  await settle(700);
  const back = await pixel(100, 300);
  check("Cancel restores the red", back[0] - back[2] > 150, String(back));

  // ---- Hue/Saturation: swatches, band bar, sample dropper
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-hue-saturation").click();
  await page.getByTestId("hue-band-bar").waitFor();
  await settle(500);
  const editor = page.getByTestId("adjustment-editor");
  await shot("10-hue-sat-master", editor);
  await page.getByTestId("hue-range-4").click();
  await settle(200);
  check("Blues shows its default band", (await readout()).replace(/\s+/g, " ") === "195° / 225° 255° / 285°", await readout());
  await page.getByTestId("hue-dropper-sample").click();
  await clickDoc(100, 200);
  await settle(600);
  check("Sampling red moves the Blues band onto red", (await readout()).replace(/\s+/g, " ") === "315° / 345° 15° / 45°", await readout());
  await page.keyboard.press("Escape");
  const sat = page.getByTestId("hue-saturation").locator('[role="slider"]');
  const sb = await sat.boundingBox();
  await page.mouse.click(sb.x + 1, sb.y + sb.height / 2);
  await settle(900);
  const red = await pixel(100, 200);
  const blue = await pixel(300, 200);
  check("Red is now grey (Blues range, band on red, saturation −100)", Math.max(...red) - Math.min(...red) <= 3, String(red));
  check("Blue is untouched", near(blue, [30, 40, 220], 3), String(blue));
  await shot("11-hue-sat-band-on-red", editor);

  // Drag the falloff-end handle right: the readout follows.
  await page.getByTestId("hue-band-handle-3").scrollIntoViewIfNeeded();
  const h3 = await page.getByTestId("hue-band-handle-3").boundingBox();
  await page.mouse.move(h3.x + h3.width / 2, h3.y + 4);
  await page.mouse.down();
  await page.mouse.move(h3.x + h3.width / 2 + 25, h3.y + 4, { steps: 6 });
  await page.mouse.up();
  await settle(500);
  const r2 = (await readout()).replace(/\s+/g, " ");
  check("Dragging the falloff handle widens the band", r2.startsWith("315° / 345° 15° /") && !r2.endsWith("/ 45°"), r2);

  await page.getByTestId("hue-invert").click();
  await settle(700);
  const redInv = await pixel(100, 200);
  const blueInv = await pixel(300, 200);
  check("Invert swaps the range: red back, blue greyed", Math.max(...redInv) - Math.min(...redInv) > 150 && Math.max(...blueInv) - Math.min(...blueInv) <= 3, `${redInv} ${blueInv}`);
  await shot("12-hue-sat-inverted", editor);

  // ---- Targeted adjust on a fresh Hue/Saturation layer
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-hue-saturation").click();
  await settle(600);
  await page.getByTestId("hue-target").click();
  // The layer below has greyed the blue, so drag on red.
  const b0 = client(100, 100);
  await page.mouse.move(b0.x, b0.y);
  await page.mouse.down();
  await page.mouse.move(b0.x + 80, b0.y, { steps: 10 });
  await page.mouse.up();
  await settle(800);
  await page.keyboard.press("Escape");
  const redSel = await page.getByTestId("hue-range-0").getAttribute("aria-checked");
  check("Targeted drag on red picks Reds", redSel === "true", redSel);
  check("…and raises its saturation by 40", (await sliderValue("hue-saturation")) === "40", await sliderValue("hue-saturation"));
  await shot("13-hue-sat-targeted", editor);
  await shot("14-hue-sat-full");

  // ---- Levels gray dropper on the warm patch (samples beneath the layer)
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-levels").click();
  await page.getByTestId("levels-dropper-gray").waitFor();
  await settle(500);
  const warmBefore = await pixel(500, 100);
  await page.getByTestId("levels-dropper-gray").click();
  await clickDoc(500, 100);
  await settle(900);
  const warm = await pixel(500, 100);
  check("Gray dropper neutralises the warm patch", Math.abs(warm[0] - warm[1]) <= 2 && Math.abs(warm[1] - warm[2]) <= 2, `${warmBefore} → ${warm}`);
  await page.getByTestId("levels-dropper-white").click();
  await clickDoc(500, 100);
  await settle(900);
  const white = await pixel(500, 100);
  check("White dropper makes the patch white", white.every((v) => v >= 250), String(white));
  await page.keyboard.press("Escape");
  await shot("20-levels-droppers", page.getByTestId("adjustment-editor"));
  await shot("21-levels-full");
  // Take the Levels layer back out (white pick, gray pick, the layer).
  for (let i = 0; i < 3; i++) {
    await page.keyboard.press(`${MOD}+z`);
    await settle(300);
  }

  // ---- Grain
  await page.getByTestId("tab-adjustments").click();
  await page.getByTestId("adjustments-grain").click();
  await page.getByTestId("grain-amount").waitFor();
  await settle(900);
  const g = [];
  for (let i = 0; i < 10; i++) g.push((await pixel(420 + i * 15, 300))[0]);
  check("Grain adds neutral noise to the grey patch", new Set(g).size > 3, JSON.stringify(g));
  const gp = await pixel(430, 310);
  check("Grain stays neutral", Math.abs(gp[0] - gp[1]) <= 1 && Math.abs(gp[1] - gp[2]) <= 1, String(gp));
  await shot("30-grain-panel", page.getByTestId("adjustment-editor"));
  const z = client(480, 300);
  await page.screenshot({ path: `${OUT}/31-grain-canvas.png`, clip: { x: z.x - 120, y: z.y - 80, width: 240, height: 160 } });

  check("No page errors", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("run", false, String(e?.stack ?? e) + " | errors: " + errors.join(" | "));
  await page.screenshot({ path: `${OUT}/zz-failure.png` });
}
await browser.close();
process.exit(failed ? 1 : 0);
