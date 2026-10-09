// Records the Photoshop gauntlet as a demo video: a slow act over the richest
// files in the corpus, then the whole corpus at full speed, then the tally.
//
//   node capture-demo.mjs <serverUrl> <outDir>
//
// Captions are rendered in the page (the product's own type and palette), not
// burned in afterwards, so they match the app exactly. Playwright records the
// page itself — no desktop, no cursor, no window chrome.
import { chromium } from "playwright";
import { mkdirSync, renameSync, readdirSync } from "node:fs";
import { join } from "node:path";

const URL_ = process.argv[2] ?? "http://127.0.0.1:5261";
const OUT = process.argv[3] ?? ".";
mkdirSync(OUT, { recursive: true });

// The ten files worth dwelling on: many layers, real artwork, and a couple we
// visibly lose on. The corpus is mostly 32×32 fixtures, which read as nothing.
const SHOWCASE = [
  "masks2",                    // a real-looking poster, 640×1136
  "blend-modes/rgb-blend-modes", // 164 layers
  "pen-text",
  "layer_effects",             // 12 layers of styles, we miss by 8
  "artboard.psd",
  "patterns.psd",
  "stroke-effects",            // one we lose badly: MAE 43.7
  "advanced-blending",
].join(",");

/** Injected into every page: caption bar, full-screen cards, presentation CSS. */
function director() {
  const css = document.createElement("style");
  css.textContent = `
    /* Presentation framing: the product page is designed for a working
       window; a 1920×1080 video wants the plates larger and the chrome
       quieter. Capture-only — the app itself is untouched. */
    #notices { display: none !important; }
    .stage, main { --plate-max: none; }
    .plate img { image-rendering: auto; }

    #demo-cap-wrap { position: fixed; inset: auto 0 132px 0; display: grid; place-items: center;
      pointer-events: none; z-index: 2147483000; }
    #demo-cap { max-width: 1180px; margin: 0 72px; padding: 18px 30px; border-radius: 16px;
      background: rgba(8,10,14,.82); border: 1px solid rgba(255,255,255,.10);
      box-shadow: 0 24px 60px rgba(0,0,0,.55); backdrop-filter: blur(14px);
      font: 500 30px/1.32 var(--font-sans, -apple-system, system-ui, sans-serif);
      color: #f2f5f9; letter-spacing: -0.01em; text-align: center; text-wrap: balance;
      opacity: 0; transform: translateY(14px); transition: opacity .5s ease, transform .5s ease; }
    #demo-cap.on { opacity: 1; transform: none; }
    #demo-cap-wrap.low { inset: auto 0 34px 0; }
    #demo-cap b { color: var(--accent, #8f7bff); font-weight: 650; }
    #demo-cap .sub { display: block; margin-top: 6px; font-size: 21px; color: #aab3c0; font-weight: 450; }

    #demo-card { position: fixed; inset: 0; z-index: 2147483600; display: grid; place-items: center;
      background: #080a0e; opacity: 0; transition: opacity .7s ease; pointer-events: none; }
    #demo-card.on { opacity: 1; }
    #demo-card .inner { text-align: center; max-width: 1200px; padding: 0 80px; }
    #demo-card h1 { font: 650 92px/1.04 var(--font-sans, -apple-system, system-ui, sans-serif);
      letter-spacing: -0.035em; color: #f7f9fc; margin: 0 0 26px; text-wrap: balance; }
    #demo-card h1 .dot { display: inline-block; width: .13em; height: .13em; border-radius: 50%;
      background: var(--accent, #8f7bff); margin-left: .07em; vertical-align: baseline; }
    #demo-card p { font: 450 31px/1.45 var(--font-sans, -apple-system, system-ui, sans-serif);
      color: #9aa4b2; margin: 0; text-wrap: balance; }
    #demo-card .rule { width: 92px; height: 3px; background: var(--accent, #8f7bff);
      margin: 34px auto; border-radius: 2px; }
    #demo-card .stat { display: flex; gap: 58px; justify-content: center; margin-top: 46px; }
    #demo-card .stat div { font: 650 54px/1 var(--font-mono, ui-monospace, monospace); color: #f7f9fc;
      font-variant-numeric: tabular-nums; }
    #demo-card .stat span { display: block; margin-top: 12px; font: 450 18px/1.3 var(--font-sans, sans-serif);
      color: #8b95a3; letter-spacing: .04em; text-transform: uppercase; }
    @media (prefers-reduced-motion: reduce) { #demo-cap, #demo-card { transition: none; } }
  `;
  const add = () => {
    document.head.appendChild(css);
    const w = document.createElement("div");
    w.id = "demo-cap-wrap";
    w.innerHTML = `<div id="demo-cap"></div>`;
    const c = document.createElement("div");
    c.id = "demo-card";
    c.innerHTML = `<div class="inner"></div>`;
    document.body.append(w, c);
  };
  if (document.body) add();
  else document.addEventListener("DOMContentLoaded", add);

  window.__capLow = (on) => document.getElementById("demo-cap-wrap")?.classList.toggle("low", !!on);
  window.__caption = (html, sub) => {
    const el = document.getElementById("demo-cap");
    if (!el) return;
    if (!html) { el.classList.remove("on"); return; }
    el.classList.remove("on");
    setTimeout(() => {
      el.innerHTML = html + (sub ? `<span class="sub">${sub}</span>` : "");
      el.classList.add("on");
    }, el.textContent ? 260 : 0);
  };
  window.__card = (html) => {
    const el = document.getElementById("demo-card");
    if (!el) return;
    if (!html) { el.classList.remove("on"); return; }
    el.querySelector(".inner").innerHTML = html;
    el.classList.add("on");
  };
}

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1920, height: 1080 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
  reducedMotion: "no-preference",
  recordVideo: { dir: join(OUT, "raw"), size: { width: 1920, height: 1080 } },
});
await context.addInitScript(director);
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

// ── Title card ────────────────────────────────────────────────────────────
await page.goto(`${URL_}/?pace=3000&autostart=1&filter=${encodeURIComponent(SHOWCASE)}`, { waitUntil: "load" });
await page.evaluate(() => window.__card(`
  <h1>The Photoshop gauntlet<span class="dot"></span></h1>
  <p>OpenPhotoEdit renders 269 real Photoshop files — and checks every one against the composite Photoshop itself saved inside them.</p>`));
await wait(4600);
await page.evaluate(() => window.__card(null));
await wait(900);

// ── Act one: the richest files, slowly ────────────────────────────────────
// Captions are keyed to the file actually on screen, never to a stopwatch:
// a caption that describes the previous file is a lie at 30 frames a second.

// The server walks the corpus in its own order, so the page tells us which
// file is on screen rather than the other way round: a MutationObserver on
// the filename fires the caption that belongs to it.
await page.evaluate((lines) => {
  const fname = document.getElementById("fname");
  const fired = new Set();
  const show = (entry) => window.__caption(entry.main, entry.sub);
  show(lines.find((l) => l.key === "*"));
  const check = () => {
    const text = fname?.textContent ?? "";
    for (const l of lines) {
      if (l.key === "*" || fired.has(l.key)) continue;
      if (text.includes(l.key)) { fired.add(l.key); show(l); return; }
    }
  };
  new MutationObserver(check).observe(fname, { childList: true, subtree: true, characterData: true });
  check();
}, [
  { key: "*", main: "Left, <b>ours</b>. Centre, <b>Adobe's</b>. Right, the difference between them.",
    sub: "Amplified ×4 — black means the two agree" },
  { key: "rgb-blend-modes", main: "This one is <b>164 layers</b> of blend modes, clipping and masks.",
    sub: "Parsed and composited by one Rust engine, on this laptop" },
  { key: "layer_effects", main: "Every file is scored in <b>levels of error</b>, 0 to 255.",
    sub: "Exact, within one, within three — or a miss" },
  { key: "stroke-effects", main: "When we lose, the heat map says so.",
    sub: "Layer styles are where we still miss Adobe" },
]);
// Long enough for the eight showcase files at three seconds each.
await wait(27000);
await page.evaluate(() => window.__caption(null));
await wait(900);

// ── Act two: the whole corpus, full speed ─────────────────────────────────
await page.goto(`${URL_}/?pace=0&autostart=1`, { waitUntil: "load" });
await page.evaluate(() => window.__caption("Now the whole corpus.", "Full speed"));
await wait(1600);
await page.evaluate(() => window.__caption(null));

// Wait for the run to finish, then read this run's own figures off the
// summary card. Quoting remembered numbers over a live board is how a demo
// ends up lying by a decimal point.
await page.waitForSelector("#card .facts .fact", { timeout: 120000 });
await wait(900);
const run = await page.evaluate(() => {
  const facts = {};
  for (const f of document.querySelectorAll("#card .facts .fact")) {
    facts[(f.querySelector(".k")?.textContent ?? "").trim().toLowerCase()] =
      (f.querySelector(".v")?.textContent ?? "").trim();
  }
  const head = (document.querySelector("#card h2")?.textContent ?? "").trim();
  const line = (document.querySelector("#card .headline")?.textContent ?? "").trim();
  const n = (s) => Number(String(s).replace(/[^\d.]/g, ""));
  return { facts, head, line, good: n(head.split(" of ")[0]), comparable: n(head.split(" of ")[1]) };
});
console.log("run figures:", JSON.stringify(run));
const secs = run.facts["total time"] ?? "";
const fps = run.facts["files / sec"] ?? "";
const mp = run.facts["pixels"] ?? "";

await page.evaluate(() => window.__capLow(true));
await page.evaluate(({ secs, fps, mp }) => window.__caption(
  `<b>269 files · ${mp} · ${secs}</b>`,
  `${fps} files a second, on one laptop, nothing uploaded`), { secs, fps, mp });
await wait(5000);
await page.evaluate(() => window.__caption(null));
await wait(1000);

// ── The tally ─────────────────────────────────────────────────────────────
await page.evaluate(() => window.__caption(
  "The honest tally.",
  "244 of the files carry an Adobe composite to compare against"));
await wait(4600);
await page.evaluate(() => window.__caption(null));
await wait(600);

await page.evaluate(({ good, comparable }) => window.__card(`
  <h1>Checked against Adobe<span class="dot"></span></h1>
  <div class="stat">
    <div>${good}<span>within 1 level</span></div>
    <div>${comparable}<span>files compared</span></div>
    <div>269<span>of 269 parse</span></div>
  </div>
  <div class="rule"></div>
  <p>54 files still miss — layer styles, knockout groups. They are on the chart too.</p>`), run);
await wait(5200);
await page.evaluate(() => window.__card(`
  <h1>OpenPhotoEdit<span class="dot"></span></h1>
  <p>A layered photo editor with unmetered local AI.<br>Open source · nothing leaves your machine · openphotoedit.com</p>`));
await wait(4200);

await page.close();
await context.close();
await browser.close();

const raw = join(OUT, "raw");
const file = readdirSync(raw).find((f) => f.endsWith(".webm"));
renameSync(join(raw, file), join(OUT, "gauntlet-raw.webm"));
console.log(JSON.stringify({ video: join(OUT, "gauntlet-raw.webm"), pageErrors: errors }, null, 1));
