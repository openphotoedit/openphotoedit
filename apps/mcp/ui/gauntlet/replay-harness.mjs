/**
 * replay-harness.mjs — a fake AG-UI server for apps/mcp/ui/gauntlet/index.html.
 *
 * The Rust server may or may not be running; this page has to be provable
 * without it. So the harness:
 *
 *   - serves index.html from a synthetic origin through page.route, under a
 *     CSP as strict as the one the real server may send, and watches for
 *     securitypolicyviolation,
 *   - replaces window.EventSource with a fake one before the page's script
 *     runs, so the page goes down its real transport path and never knows,
 *   - generates every pixel it "receives" inside the page with OffscreenCanvas
 *     — ours, Adobe's, and the amplified difference — so the harness depends
 *     on no file in the repo but index.html itself,
 *   - replays a 40-file run: mostly good, two ugly outliers, one file with no
 *     Adobe composite, one outright failure, then the summary,
 *   - drives mid-run, an outlier landing, a worst-offender click, pause and
 *     resume, the summary card, a dropped connection, an empty filter and a
 *     failed run, screenshotting each one wide and narrow, light and dark.
 *
 * Run:  node apps/mcp/ui/gauntlet/replay-harness.mjs
 * Out:  apps/mcp/ui/gauntlet/shots/*.png  + a pass/fail summary on stdout
 *
 * Against the real server instead:
 *       node apps/mcp/ui/gauntlet/replay-harness.mjs --live http://127.0.0.1:8787
 */

import { readFileSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const HERE = dirname(fileURLToPath(import.meta.url));
const SHOTS = join(HERE, "shots");
const PAGE = join(HERE, "index.html");
const ORIGIN = "https://gauntlet.test";
const DOC = ORIGIN + "/";

/* Playwright is not vendored in this folder. Look for it where the suite
   keeps it — the web app first, then any sibling app under openapps/, then
   whatever the ambient resolver can find. */
function loadPlaywright() {
  const tried = [];
  const roots = [
    join(HERE, "../../../web/package.json"),
    join(HERE, "../../../../../openmarkdown/package.json"),
    join(HERE, "../../../../../opendownloader/package.json"),
    join(HERE, "../../../../../opennotetaker/package.json"),
    join(HERE, "../../../../../openvidcut/package.json"),
    join(HERE, "package.json")
  ];
  for (const r of roots) {
    try { return createRequire(r)("playwright"); }
    catch (e) { tried.push(r); }
  }
  throw new Error("playwright not found. Tried:\n  " + tried.join("\n  ") +
    "\nInstall it in apps/web (npm i -D playwright) or run with NODE_PATH set.");
}
const { chromium } = loadPlaywright();

const argv = process.argv.slice(2);
const LIVE = argv.includes("--live") ? argv[argv.indexOf("--live") + 1] : null;

/* A policy at least as tight as anything the server is likely to send.
   connect-src 'self' is what the SSE stream needs; nothing else is allowed. */
const CSP =
  "default-src 'none'; " +
  "script-src 'self' 'unsafe-inline'; " +
  "style-src 'self' 'unsafe-inline'; " +
  "img-src 'self' data:; " +
  "connect-src 'self'; " +
  "base-uri 'none'; form-action 'none';";

/* ===================================================================== */
/* The fake stream + the pixel factory, both installed into the page.     */
/* ===================================================================== */

function pageInit() {
  /* ---------------------------------------------------- fake EventSource */
  const F = {
    sources: [],
    last: null,
    opened: [],
    emit(obj) {
      const s = F.last;
      if (!s || s.readyState === 2) return;
      const ev = { data: JSON.stringify(obj) };
      for (const fn of s._msg) fn(ev);
      if (s.onmessage) s.onmessage(ev);
    },
    /** closed=true → the server is gone for good; false → a transient blip. */
    drop(closed) {
      const s = F.last;
      if (!s) return;
      s.readyState = closed ? 2 : 0;
      if (s.onerror) s.onerror({});
    }
  };

  class FakeEventSource {
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this._msg = [];
      this._named = {};
      F.sources.push(this);
      F.opened.push(url);
      F.last = this;
      setTimeout(() => {
        if (this.readyState === 2) return;
        this.readyState = 1;
        if (this.onopen) this.onopen({});
      }, 0);
    }
    addEventListener(name, fn) {
      if (name === "message") this._msg.push(fn);
      else (this._named[name] = this._named[name] || []).push(fn);
    }
    removeEventListener() {}
    close() { this.readyState = 2; }
  }
  FakeEventSource.CONNECTING = 0;
  FakeEventSource.OPEN = 1;
  FakeEventSource.CLOSED = 2;
  window.EventSource = FakeEventSource;
  window.__fake = F;

  /* ------------------------------------------------------ CSP + error log */
  window.__csp = [];
  window.__errs = [];
  addEventListener("securitypolicyviolation", (e) =>
    window.__csp.push(e.violatedDirective + " " + e.blockedURI));
  addEventListener("error", (e) => window.__errs.push(String(e.message)));

  /* ------------------------------------------------------- pixel factory */
  /* the preview keeps the file's own shape, the way the server's would */
  function fit(w, h) {
    const long = 256;
    if (!(w > 0) || !(h > 0)) return [256, 192];
    const k = long / Math.max(w, h);
    return [Math.max(32, Math.round(w * k)), Math.max(32, Math.round(h * k))];
  }

  function rng(seed) {
    let s = (seed * 2654435761) >>> 0;
    return () => { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; };
  }

  /** A plausible layered composite: sky, ground, a subject, a caption bar. */
  function scene(g, seed, W, H) {
    const r = rng(seed);
    const hue = Math.floor(r() * 360);
    const sky = g.createLinearGradient(0, 0, 0, H);
    sky.addColorStop(0, "hsl(" + hue + ",58%," + (22 + r() * 26).toFixed(0) + "%)");
    sky.addColorStop(1, "hsl(" + ((hue + 40) % 360) + ",48%," + (60 + r() * 25).toFixed(0) + "%)");
    g.fillStyle = sky;
    g.fillRect(0, 0, W, H);

    g.fillStyle = "hsl(" + ((hue + 180) % 360) + ",30%,22%)";
    g.beginPath();
    g.moveTo(0, H);
    for (let x = 0; x <= W; x += 16) g.lineTo(x, H * (0.62 + 0.16 * Math.sin(x / 26 + seed)));
    g.lineTo(W, H);
    g.closePath();
    g.fill();

    for (let i = 0; i < 4 + Math.floor(r() * 4); i++) {
      g.globalAlpha = 0.35 + r() * 0.5;
      g.fillStyle = "hsl(" + Math.floor(r() * 360) + ",70%," + (40 + r() * 40).toFixed(0) + "%)";
      const w = 18 + r() * 70, h = 18 + r() * 70;
      if (r() > 0.5) g.fillRect(r() * (W - w), r() * (H - h), w, h);
      else { g.beginPath(); g.arc(r() * W, r() * H, w / 2, 0, Math.PI * 2); g.fill(); }
    }
    g.globalAlpha = 1;

    g.fillStyle = "rgba(0,0,0,0.55)";
    g.fillRect(0, H - 26, W, 26);
    g.fillStyle = "rgba(255,255,255,0.85)";
    for (let i = 0, x = 10; i < 7; i++) {
      const w = 8 + r() * 26;
      g.fillRect(x, H - 18, w, 7);
      x += w + 5;
      if (x > W - 20) break;
    }
  }

  function toURL(canvas) {
    return canvas.convertToBlob({ type: "image/jpeg", quality: 0.74 }).then((b) =>
      new Promise((res) => {
        const fr = new FileReader();
        fr.onload = () => res(fr.result);
        fr.readAsDataURL(b);
      }));
  }

  /**
   * ours / adobe / diff for one file.
   *  mae is the target mean absolute error in 8-bit levels. Small errors are
   *  spread as noise (rounding); large ones are a region that blended wrong,
   *  which is what a real blend-mode bug actually looks like.
   */
  window.__pixels = async function (seed, mae, fw, fh) {
    const [W, H] = fit(fw, fh);
    const a = new OffscreenCanvas(W, H), ga = a.getContext("2d");
    scene(ga, seed, W, H);
    const ours = ga.getImageData(0, 0, W, H);

    const b = new OffscreenCanvas(W, H), gb = b.getContext("2d");
    gb.putImageData(ours, 0, 0);
    if (mae > 10) {
      const r = rng(seed + 991);
      gb.globalCompositeOperation = r() > 0.5 ? "overlay" : "color-dodge";
      gb.globalAlpha = Math.min(0.85, mae / 60);
      gb.fillStyle = "hsl(" + Math.floor(r() * 360) + ",80%,55%)";
      gb.fillRect(W * 0.12, H * 0.1, W * (0.4 + r() * 0.4), H * (0.4 + r() * 0.45));
      gb.globalCompositeOperation = "source-over";
      gb.globalAlpha = 1;
    }
    const adobe = gb.getImageData(0, 0, W, H);

    /* spread the remaining error as rounding noise so the mean lands near mae */
    const noise = Math.min(mae, 10);
    if (noise > 0) {
      const r = rng(seed + 17);
      const d = adobe.data;
      for (let i = 0; i < d.length; i += 4) {
        for (let k = 0; k < 3; k++) {
          const n = (r() * 2 - 1) * noise * 2.2;
          d[i + k] = Math.max(0, Math.min(255, d[i + k] + n));
        }
      }
      gb.putImageData(adobe, 0, 0);
    }

    /* the amplified absolute difference */
    const c = new OffscreenCanvas(W, H), gc = c.getContext("2d");
    const out = gc.createImageData(W, H);
    const A = ours.data, B = adobe.data, D = out.data;
    /* a gamma boost rather than a linear gain: one level of error is visible
       and forty levels still have shape instead of clipping to white */
    for (let i = 0; i < A.length; i += 4) {
      for (let k = 0; k < 3; k++) {
        const d = Math.abs(A[i + k] - B[i + k]) / 255;
        D[i + k] = d === 0 ? 0 : Math.min(255, Math.round(255 * Math.pow(d, 0.42)));
      }
      D[i + 3] = 255;
    }
    gc.putImageData(out, 0, 0);

    const [u1, u2, u3] = await Promise.all([toURL(a), toURL(b), toURL(c)]);
    return { ours: u1, adobe: u2, diff: u3 };
  };
}

/* ===================================================================== */
/* The replay script — the run the fake server performs.                  */
/* ===================================================================== */

/** 40 files: a believable slice of the corpus. */
const FILES = [
  ["psd-tools/1layer.psd", 800, 600, 1, 0],
  ["psd-tools/2layers.psd", 800, 600, 2, 0.31],
  ["psd-tools/group.psd", 1024, 768, 6, 0.44],
  ["psd-tools/hidden-groups.psd", 1024, 768, 9, 0.52],
  ["psd-tools/layer_effects.psd", 800, 600, 12, 0.42],
  ["psd-tools/layer_mask.psd", 900, 640, 4, 0.18],
  ["psd-tools/vector-mask.psd", 900, 640, 5, 1.7],
  ["psd-tools/clipping-mask.psd", 1200, 900, 8, 0.61],
  ["psd-tools/smart-object.psd", 1600, 1200, 3, 0.77],
  ["psd-tools/text-simple.psd", 800, 600, 2, 0],
  ["photoshop-api/blend-multiply.psd", 1400, 1050, 7, 0.28],
  ["photoshop-api/blend-screen.psd", 1400, 1050, 7, 0.33],
  ["photoshop-api/blend-overlay.psd", 1400, 1050, 7, 2.4],
  ["photoshop-api/blend-softlight.psd", 1400, 1050, 7, 41.2],   // outlier one
  ["photoshop-api/blend-hardmix.psd", 1400, 1050, 7, 3.9],
  ["photoshop-api/adjust-curves.psd", 2000, 1333, 5, 0.47],
  ["photoshop-api/adjust-levels.psd", 2000, 1333, 5, 0.39],
  ["photoshop-api/adjust-huesat.psd", 2000, 1333, 6, 1.26],
  ["corpus/portrait-retouch.psd", 3000, 2000, 24, 0.55],
  ["corpus/product-cutout.psd", 2400, 2400, 11, 0.21],
  ["corpus/poster-cmyk.psd", 2480, 3508, 18, null],             // structure only
  ["corpus/landscape-hdr.psd", 4000, 2667, 9, 0.66],
  ["corpus/duotone-cover.psd", 1800, 2400, 6, 5.8],
  ["corpus/pattern-fill.psd", 1200, 1200, 4, 0.9],
  ["corpus/gradient-map.psd", 1600, 1000, 5, 1.05],
  ["corpus/drop-shadow-stack.psd", 1500, 1000, 16, 2.1],
  ["corpus/inner-glow.psd", 1500, 1000, 13, 27.6],              // outlier two
  ["corpus/stroke-effect.psd", 1500, 1000, 10, 4.4],
  ["corpus/16bit-scan.psd", 3400, 2200, 3, 0.74],
  ["corpus/grayscale-plate.psd", 2000, 1400, 2, 0.12],
  ["corpus/truncated.psd", 0, 0, 0, undefined],                 // failure
  ["corpus/many-layers.psd", 1920, 1080, 64, 1.48],
  ["corpus/nested-groups.psd", 1920, 1080, 38, 0.83],
  ["corpus/knockout-group.psd", 1280, 960, 9, 8.6],
  ["corpus/passthrough.psd", 1280, 960, 12, 1.92],
  ["corpus/layer-comp.psd", 1600, 1200, 21, 0.58],
  ["corpus/transparency-shapes.psd", 1100, 1100, 7, 0.36],
  ["corpus/big-canvas.psd", 6000, 4000, 4, 0.69],
  ["corpus/cmyk-spot.psd", 2100, 2970, 8, 12.4],
  ["corpus/final-proof.psd", 2400, 1600, 15, 0.29]
];

const OUTLIER_A = 13;     // blend-softlight, MAE 41.2
const STRUCTURE = 20;     // poster-cmyk, no Adobe composite
const OUTLIER_B = 26;     // inner-glow, MAE 27.6
const FAILED = 30;        // truncated

const TOTAL = FILES.length;
const COMPARABLE = FILES.filter((f) => typeof f[4] === "number").length;

function verdictOf(mae) {
  if (mae === null) return "structureOnly";
  if (mae === undefined) return "failed";
  if (mae === 0) return "exact";
  if (mae <= 1) return "within1";
  if (mae <= 3) return "within3";
  if (mae <= 10) return "within10";
  return "over";
}

/** Everything the server would have to compute, computed here once. */
function plan() {
  const tally = { exact: 0, within1: 0, within3: 0, within10: 0, over: 0, structureOnly: 0, failed: 0 };
  const hist = new Array(32).fill(0);
  const worst = [];
  let elapsed = 0, mp = 0;
  const steps = [];
  for (let i = 0; i < TOTAL; i++) {
    const [file, w, h, layers, mae] = FILES[i];
    const v = verdictOf(mae);
    tally[v]++;
    if (typeof mae === "number") {
      hist[Math.min(31, Math.floor(mae / 0.5))]++;
      worst.push({ file, mae, verdict: v, idx: i });
    }
    const ms = Math.max(6, Math.round((w * h) / 1e6 * 34 + 9));
    elapsed += ms + 4;
    mp += (w * h) / 1e6;
    steps.push({
      i, file, w, h, layers, mae, verdict: v, ms,
      bucket: typeof mae === "number" ? Math.min(31, Math.floor(mae / 0.5)) : -1,
      tally: Object.assign({}, tally),
      hist: hist.slice(),
      worst: worst.slice().sort((a, b) => b.mae - a.mae).slice(0, 8),
      throughput: {
        filesPerSec: Math.round(((i + 1) / (elapsed / 1000)) * 10) / 10,
        megapixelsPerSec: Math.round((mp / (elapsed / 1000)) * 10) / 10,
        elapsedMs: elapsed
      }
    });
  }
  return steps;
}
const STEPS = plan();

/* ===================================================================== */
/* Driving                                                                */
/* ===================================================================== */

let failures = 0;
const notes = [];
function check(ok, label, detail) {
  if (ok) notes.push("  ok   " + label);
  else { failures++; notes.push("  FAIL " + label + (detail ? " — " + detail : "")); }
}

const produced = [];
async function shot(page, name, full) {
  await page.screenshot({ path: join(SHOTS, name + ".png"), fullPage: !!full });
  produced.push(name + ".png");
}

const emit = (page, ev) => page.evaluate((e) => window.__fake.emit(e), ev);

async function emitFile(page, i) {
  const s = STEPS[i];
  await page.evaluate(async (st) => {
    let pix = { ours: null, adobe: null, diff: null };
    if (st.verdict === "structureOnly") {
      pix = await window.__pixels(st.i + 3, 0, st.w, st.h);
      pix.adobe = null; pix.diff = null;
    } else if (st.verdict !== "failed") {
      pix = await window.__pixels(st.i + 3, st.mae, st.w, st.h);
    }
    /* the worst list carries its own thumbnails, exactly as the state shape says */
    window.__thumbs = window.__thumbs || {};
    if (pix.diff) window.__thumbs[st.file] = pix;

    const worst = st.worst.map((w) => Object.assign({ file: w.file, mae: w.mae, verdict: w.verdict },
      window.__thumbs[w.file] || {}));

    /* a real server emits a mix of coarse and fine patches; so does this */
    const delta = [
      { op: "replace", path: "/index", value: st.i },
      { op: "replace", path: "/phase", value: "compare" },
      {
        op: "replace", path: "/current", value: {
          file: st.file, width: st.w, height: st.h, layers: st.layers,
          mae: st.mae === null || st.mae === undefined ? null : st.mae,
          p95: st.mae === null || st.mae === undefined ? null : Math.round(st.mae * 4.8 * 10) / 10,
          verdict: st.verdict, ms: st.ms,
          ours: pix.ours, adobe: pix.adobe, diff: pix.diff
        }
      },
      { op: "replace", path: "/tally/" + st.verdict, value: st.tally[st.verdict] },
      { op: "replace", path: "/worst", value: worst },
      { op: "replace", path: "/throughput", value: st.throughput }
    ];
    if (st.bucket >= 0) delta.push({ op: "replace", path: "/histogram/" + st.bucket, value: st.hist[st.bucket] });
    window.__fake.emit({ type: "STATE_DELTA", delta });
  }, s);
}

async function drain(page, timeout = 15000) {
  await page.waitForFunction(() => window.__gauntlet.queue === 0 && !window.__gauntlet.paused,
    null, { timeout });
  await page.waitForTimeout(520);   // let the crossfade and the counters settle
}

async function startRun(page, opts = {}) {
  await page.click("#start");
  await page.waitForFunction(() => window.__fake.last !== null, null, { timeout: 4000 });
  await emit(page, { type: "RUN_STARTED", threadId: "gauntlet", runId: "run_5f2ab19c" });
  await emit(page, { type: "STEP_STARTED", stepName: "import" });
  await emit(page, {
    type: "STATE_SNAPSHOT",
    snapshot: {
      total: opts.total != null ? opts.total : TOTAL,
      comparable: opts.comparable != null ? opts.comparable : COMPARABLE,
      index: 0, phase: "import", current: null,
      tally: { exact: 0, within1: 0, within3: 0, within10: 0, over: 0, structureOnly: 0, failed: 0 },
      histogram: new Array(32).fill(0), worst: [],
      throughput: { filesPerSec: 0, megapixelsPerSec: 0, elapsedMs: 0 }
    }
  });
  await emit(page, { type: "STEP_STARTED", stepName: "flatten" });
  await emit(page, { type: "STEP_STARTED", stepName: "compare" });
  await drain(page);
}

async function scenarios(page, tag, wide) {
  const full = !wide;

  /* ---------------------------------------------------- 1. idle, the pitch */
  await page.goto(DOC + "?pace=160", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => !!window.__gauntlet, null, { timeout: 5000 });
  await page.waitForTimeout(250);
  check(await page.locator("#veil").isVisible(), "the idle card is up before a run (" + tag + ")");
  check(await page.locator("#card [data-act='start']").isVisible(),
    "the idle card's start button is not clipped (" + tag + ")");
  check(/^idle$/i.test((await page.locator("#conn-t").innerText()).trim()),
    "connection reads idle before a run (" + tag + ")",
    await page.locator("#conn-t").innerText());
  await shot(page, "01-idle-" + tag, full);

  /* ------------------------------------------------------------ 2. mid-run */
  await startRun(page);
  check((await page.locator(".rail .t").count()) === TOTAL,
    "the rail has one tick per file (" + tag + ")",
    String(await page.locator(".rail .t").count()));
  for (let i = 0; i <= 11; i++) await emitFile(page, i);
  await drain(page);
  check(await page.locator("#veil").isHidden(), "the idle card gets out of the way (" + tag + ")");
  check((await page.locator("#fname").innerText()).includes("blend-screen.psd"),
    "the filename follows the stream (" + tag + ")", await page.locator("#fname").innerText());
  const seen = await page.locator("#sc-seen").innerText();
  check(/^12 \/ 40/.test(seen.trim()), "the scoreboard counts what has landed (" + tag + ")", seen);
  check((await page.locator("#ours-a.on, #ours-b.on").count()) === 1,
    "exactly one layer of the ours panel is showing (" + tag + ")");
  await shot(page, "02-midrun-" + tag, full);

  /* -------------------------------------------- 3. the ugly outlier landing */
  for (let i = 12; i <= OUTLIER_A; i++) await emitFile(page, i);
  await drain(page);
  const chip = (await page.locator("#chip").innerText()).trim();
  check(/Over 10/i.test(chip), "a 41-level miss is called out, not hidden (" + tag + ")", chip);
  check((await page.locator("#mae").innerText()).trim() === "41.20",
    "the error under the triptych is the real number (" + tag + ")",
    await page.locator("#mae").innerText());
  check((await page.locator("#sc-over").innerText()).trim() === "1",
    "the over-10 counter moved (" + tag + ")");
  await shot(page, "03-outlier-" + tag, full);

  /* ------------------------------------- 4. the file with no Adobe composite */
  for (let i = OUTLIER_A + 1; i <= STRUCTURE; i++) await emitFile(page, i);
  await drain(page);
  check(/Structure only/i.test(await page.locator("#chip").innerText()),
    "a file with no Adobe composite says so (" + tag + ")");
  check((await page.locator("#mae").innerText()).trim() === "—",
    "no error is invented for a file we cannot compare (" + tag + ")");
  check(await page.locator("#plate-adobe").evaluate((e) => e.classList.contains("empty")),
    "Adobe's panel is visibly empty, not stale (" + tag + ")");
  check(/no adobe composite/i.test(await page.locator("#note-adobe").innerText()),
    "the empty panel says why it is empty (" + tag + ")",
    await page.locator("#note-adobe").innerText());
  await shot(page, "04-structure-only-" + tag, full);

  /* ------------------------- 5. click a worst offender to freeze and inspect */
  for (let i = STRUCTURE + 1; i <= OUTLIER_B; i++) await emitFile(page, i);
  await drain(page);
  const rows = page.locator(".wrow");
  check((await rows.count()) >= 3, "the worst list has filled (" + tag + ")");
  const topFile = await rows.first().getAttribute("data-file");
  await rows.first().click();
  await page.waitForTimeout(600);
  check(await page.locator("#frozen").isVisible(), "clicking a worst offender freezes it (" + tag + ")");
  check((await page.locator("#fname").innerText()).includes(topFile.split("/").pop()),
    "the triptych is showing the frozen file (" + tag + ")", await page.locator("#fname").innerText());
  check(await rows.first().evaluate((e) => e.getAttribute("aria-pressed") === "true"),
    "the frozen row reads as pressed (" + tag + ")");
  const clash = await page.evaluate(() => {
    const f = document.getElementById("frozen").getBoundingClientRect();
    const c = document.querySelector(".panel .cap").getBoundingClientRect();
    return !(f.bottom <= c.top || f.top >= c.bottom || f.right <= c.left || f.left >= c.right);
  });
  check(!clash, "the frozen banner does not cover the panel captions (" + tag + ")");
  await shot(page, "05-frozen-" + tag, full);
  await page.click("#unfreeze");
  await page.waitForTimeout(250);
  check(await page.locator("#frozen").isHidden(), "back to live releases the freeze (" + tag + ")");

  /* -------------------------------------------------------- 6. pause buffers */
  await page.click("#pause");
  await page.waitForTimeout(120);
  for (let i = OUTLIER_B + 1; i <= OUTLIER_B + 6; i++) await emitFile(page, i);
  await page.waitForTimeout(350);
  const q = await page.evaluate(() => window.__gauntlet.queue);
  check(q >= 5, "pausing holds the events instead of dropping them (" + tag + ")", "queue=" + q);
  check(/Resume/.test(await page.locator("#pause-t").innerText()),
    "the button offers to resume (" + tag + ")");
  await shot(page, "06-paused-" + tag, full);
  await page.click("#pause");
  await drain(page);
  check((await page.evaluate(() => window.__gauntlet.queue)) === 0,
    "resuming catches up (" + tag + ")");

  /* ------------------------------------------------- 7. the end, and the card */
  for (let i = OUTLIER_B + 7; i < TOTAL; i++) await emitFile(page, i);
  await drain(page);
  await emit(page, { type: "RUN_FINISHED" });
  await page.waitForTimeout(900);
  check(await page.locator("#veil").isVisible(), "the summary card lands (" + tag + ")");
  const card = await page.locator("#card").innerText();
  check(/Run complete/i.test(card), "the summary says the run is complete (" + tag + ")");
  check(/median mae/i.test(card) && /worst mae/i.test(card) && /total time/i.test(card) && /megapixels/i.test(card),
    "the summary carries median, worst, time and megapixels (" + tag + ")");
  check(/structure only/i.test(card),
    "the summary counts the uncomparable file (" + tag + ")");
  check(/41\.2/.test(card), "the summary names the worst miss (" + tag + ")");
  check(new RegExp(String(TOTAL) + " files attempted").test(card),
    "the summary counts every file attempted (" + tag + ")");
  check(await page.locator("#card [data-act='start']").isVisible(),
    "the summary card's buttons are reachable (" + tag + ")");
  await shot(page, "07-summary-" + tag, full);

  /* ------------------------------------------------- 8. the connection drops */
  await page.locator("#card [data-act='dismiss']").click();
  await page.waitForTimeout(200);
  await startRun(page);                       // a second run, killed part-way
  for (let i = 0; i <= 8; i++) await emitFile(page, i);
  await drain(page);
  await page.evaluate(() => window.__fake.drop(false));   // a blip first
  await page.waitForTimeout(300);
  check(/^reconnecting$/i.test((await page.locator("#conn-t").innerText()).trim()),
    "a transient blip reads as reconnecting, not dead (" + tag + ")",
    await page.locator("#conn-t").innerText());
  await page.evaluate(() => window.__fake.drop(true));    // then it is gone
  await page.waitForTimeout(450);
  check(await page.locator(".notice--error").first().isVisible(),
    "a dropped connection is visible, not silent (" + tag + ")");
  check(/^disconnected$/i.test((await page.locator("#conn-t").innerText()).trim()),
    "the connection pill goes red (" + tag + ")", await page.locator("#conn-t").innerText());
  check((await page.locator("#fname").innerText()).length > 4,
    "the last frame stays on screen after a drop (" + tag + ")");
  const noticeTop = await page.locator(".notice--error").first().evaluate((e) => e.getBoundingClientRect().top);
  const railTop = await page.locator(".railwrap").evaluate((e) => e.getBoundingClientRect().top);
  check(noticeTop < railTop, "the drop notice sits under the header, not below the fold (" + tag + ")",
    "notice=" + Math.round(noticeTop) + " rail=" + Math.round(railTop));
  await shot(page, "08-dropped-" + tag, full);

  /* --------------------------------------------------- 9. an empty filter run */
  await page.fill("#filter", "no-such-file");
  await page.waitForTimeout(80);
  await startRun(page, { total: 0, comparable: 0 });
  await emit(page, { type: "RUN_FINISHED" });
  await page.waitForTimeout(600);
  const empty = await page.locator("#card").innerText();
  check(/No file matched/i.test(empty), "an empty filter says so (" + tag + ")", empty.slice(0, 80));
  check(/no-such-file/.test(empty), "the empty card quotes the filter (" + tag + ")");
  check((await page.locator(".rail .t").count()) === 0, "the rail empties with it (" + tag + ")");
  await shot(page, "09-empty-filter-" + tag, full);

  /* ------------------------------------------------------- 10. a failed run */
  await page.fill("#filter", "");
  await page.waitForTimeout(80);
  check(/apply filter/i.test(await page.locator("#start-t").innerText()),
    "changing the filter offers to apply it rather than doing it mid-keystroke (" + tag + ")",
    await page.locator("#start-t").innerText());
  await startRun(page);
  check((await page.locator("#railcount").innerText()).trim().endsWith("/ " + TOTAL),
    "the rail count survives a restart (" + tag + ")", await page.locator("#railcount").innerText());
  for (let i = 0; i <= 5; i++) await emitFile(page, i);
  await drain(page);
  await emit(page, {
    type: "RUN_ERROR",
    message: "engine panicked flattening psd-tools/layer_mask.psd: unsupported depth 32",
    code: "E_FLATTEN"
  });
  await page.waitForTimeout(700);
  const bad = await page.locator("#card").innerText();
  check(/stopped/i.test(bad), "a failed run is shown as a failure (" + tag + ")", bad.slice(0, 80));
  check(/E_FLATTEN/.test(bad) && /unsupported depth 32/.test(bad),
    "the error card carries the server's message and code (" + tag + ")");
  check(/6 files/.test(bad), "the error card says how far it got (" + tag + ")");
  check((await page.locator("#s-phase").innerText()).trim() === "error",
    "the status line says error (" + tag + ")");
  check(/^stopped$/i.test((await page.locator("#conn-t").innerText()).trim()),
    "a failed run does not read as finished (" + tag + ")",
    await page.locator("#conn-t").innerText());
  await shot(page, "10-failed-run-" + tag, full);

  /* ------------------------------------------------- nothing broke on the way */
  const csp = await page.evaluate(() => window.__csp);
  check(csp.length === 0, "no CSP violation (" + tag + ")", csp.join(", "));
  const errs = await page.evaluate(() => window.__errs);
  check(errs.length === 0, "no uncaught error in the page (" + tag + ")", errs.join(", "));
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  check(overflow <= 0, "nothing overflows the viewport sideways (" + tag + ")", overflow + "px");
}

/* ===================================================================== */

async function run() {
  rmSync(SHOTS, { recursive: true, force: true });
  mkdirSync(SHOTS, { recursive: true });

  const html = readFileSync(PAGE, "utf8");
  const sizeKb = statSync(PAGE).size / 1024;
  check(!/<(script|link|img)[^>]+(src|href)\s*=\s*["']https?:/i.test(html),
    "no absolute http(s) reference in the page");
  check(!/@import|fonts\.googleapis/i.test(html), "no external font");

  const browser = await chromium.launch();
  const net = [];

  for (const theme of ["light", "dark"]) {
    const ctx = await browser.newContext({ colorScheme: theme, deviceScaleFactor: 2 });
    await ctx.addInitScript(pageInit);
    await ctx.route("**/*", async (route, req) => {
      const u = req.url().split("?")[0];
      if (u === DOC || u === ORIGIN + "/index.html") {
        await route.fulfill({
          status: 200,
          headers: { "content-type": "text/html; charset=utf-8", "content-security-policy": CSP },
          body: html
        });
        return;
      }
      net.push(u);
      await route.fulfill({ status: 404, body: "" });
    });

    const page = await ctx.newPage();
    page.on("request", (r) => {
      const u = r.url();
      if (u.startsWith("data:") || u === "about:blank") return;
      if (u.split("?")[0] === DOC || u.split("?")[0] === ORIGIN + "/index.html") return;
      net.push(u);
    });
    page.on("pageerror", (e) => check(false, "no page error (" + theme + ")", e.message));

    for (const [name, vp, wide] of [
      ["wide", { width: 1440, height: 900 }, true],
      ["narrow", { width: 430, height: 940 }, false]
    ]) {
      await page.setViewportSize(vp);
      await scenarios(page, name + "-" + theme, wide);
    }
    await ctx.close();
  }

  /* one projector-sized pass, so the big-screen type scale is proved too */
  {
    const ctx = await browser.newContext({ colorScheme: "dark", deviceScaleFactor: 1 });
    await ctx.addInitScript(pageInit);
    await ctx.route("**/*", async (route, req) => {
      const u = req.url();
      if (u.split("?")[0] === DOC) {
        await route.fulfill({
          status: 200,
          headers: { "content-type": "text/html; charset=utf-8", "content-security-policy": CSP },
          body: html
        });
      } else { net.push(u); await route.fulfill({ status: 404, body: "" }); }
    });
    const page = await ctx.newPage();
    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.goto(DOC + "?pace=0", { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => !!window.__gauntlet, null, { timeout: 5000 });
    await startRun(page);
    for (let i = 0; i <= OUTLIER_B; i++) await emitFile(page, i);
    await drain(page);
    await shot(page, "11-projector-midrun");
    for (let i = OUTLIER_B + 1; i < TOTAL; i++) await emitFile(page, i);
    await drain(page);
    await emit(page, { type: "RUN_FINISHED" });
    await page.waitForTimeout(900);
    await shot(page, "12-projector-summary");
    await ctx.close();
  }

  /* reduced motion must not break anything */
  {
    const ctx = await browser.newContext({ colorScheme: "light", reducedMotion: "reduce", deviceScaleFactor: 2 });
    await ctx.addInitScript(pageInit);
    await ctx.route("**/*", async (route, req) => {
      if (req.url().split("?")[0] === DOC) {
        await route.fulfill({
          status: 200,
          headers: { "content-type": "text/html; charset=utf-8", "content-security-policy": CSP },
          body: html
        });
      } else { net.push(req.url()); await route.fulfill({ status: 404, body: "" }); }
    });
    const page = await ctx.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(DOC + "?pace=60", { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => !!window.__gauntlet, null, { timeout: 5000 });
    await startRun(page);
    for (let i = 0; i <= OUTLIER_A; i++) await emitFile(page, i);
    await drain(page);
    check((await page.locator("#ours-a.on, #ours-b.on").count()) === 1,
      "the triptych still swaps with reduced motion");
    check((await page.locator("#sc-over").innerText()).trim() === "1",
      "counters land immediately with reduced motion");
    await shot(page, "13-reduced-motion");
    check((await page.evaluate(() => window.__errs)).length === 0, "no error under reduced motion");
    await ctx.close();
  }

  await browser.close();

  check(net.length === 0, "zero non-data network requests", net.slice(0, 6).join(", "));
  check(sizeKb < 110, "index.html stays small", sizeKb.toFixed(1) + " KB");

  console.log("\nThe Photoshop gauntlet — replay harness\n");
  console.log(notes.join("\n"));
  console.log("\nindex.html: " + sizeKb.toFixed(1) + " KB");
  console.log("network requests outside the document and data: URIs: " + net.length);
  console.log("screenshots: " + produced.length + " in " + SHOTS);
  console.log(failures === 0 ? "\nALL CHECKS PASSED\n" : "\n" + failures + " CHECK(S) FAILED\n");
  process.exit(failures === 0 ? 0 : 1);
}

/* --------------------------------------------------------------- live mode */
/* Against the real Rust server: no fake anything, the page's own EventSource
   talking to /gauntlet/stream. Proves the state shape, the numbers and that
   nothing is fetched off-origin. */

async function live(base) {
  mkdirSync(SHOTS, { recursive: true });
  const browser = await chromium.launch();
  const off = [];
  const shots = [];

  async function session(theme, vp, tag, pace) {
    const ctx = await browser.newContext({ colorScheme: theme, deviceScaleFactor: 2 });
    const page = await ctx.newPage();
    page.on("request", (r) => {
      const u = r.url();
      if (!u.startsWith("data:") && !u.startsWith(base)) off.push(u);
    });
    page.on("pageerror", (e) => check(false, "no page error (" + tag + ")", e.message));
    await page.setViewportSize(vp);
    await page.goto(base + "/?pace=" + pace, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => !!window.__gauntlet, null, { timeout: 10000 });
    await page.click("#start");
    await page.waitForFunction(() => window.__gauntlet.conn === "live", null, { timeout: 15000 })
      .catch(() => check(false, "the page connects to the real stream (" + tag + ")"));
    check(/gauntlet\/stream/.test(await page.locator("#s-src").innerText()),
      "it found the stream on the first candidate route (" + tag + ")",
      await page.locator("#s-src").innerText());
    await page.waitForFunction(() => (window.__gauntlet.ledger || []).length > 20,
      null, { timeout: 90000 })
      .catch(() => check(false, "files arrive from the real stream (" + tag + ")"));
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, "live-midrun-" + tag + ".png") });
    shots.push("live-midrun-" + tag + ".png");

    await page.waitForFunction(() => window.__gauntlet.ended === "done", null, { timeout: 600000 })
      .catch(() => check(false, "the real run reaches the summary (" + tag + ")"));
    await page.waitForTimeout(1500);
    await page.screenshot({ path: join(SHOTS, "live-summary-" + tag + ".png") });
    shots.push("live-summary-" + tag + ".png");

    const st = await page.evaluate(() => window.__gauntlet.state);
    const card = await page.locator("#card").innerText();
    const conn = (await page.locator("#conn-t").innerText()).trim();
    await ctx.close();
    return { st, card, conn };
  }

  const wide = await session("dark", { width: 1440, height: 900 }, "wide-dark", 14);

  /* the two bugs the server author reported */
  check(/^finished$/i.test(wide.conn),
    "the pill says finished after RUN_FINISHED, not reconnecting", wide.conn);
  const mp = /PIXELS\s*([\d.,]+)/i.exec(wide.card.replace(/\s+/g, " "));
  const mpNum = mp ? Number(mp[1].replace(/,/g, "")) : NaN;
  check(mpNum >= 40 && mpNum <= 55,
    "the megapixel figure matches the corpus (46.6), not double it", String(mpNum));

  /* and the rest of the numbers */
  const t = wide.st.tally || {};
  const sum = Object.keys(t).reduce((a, k) => a + t[k], 0);
  check(wide.st.total === 269, "269 files in the corpus", String(wide.st.total));
  check(sum === 269, "every file is tallied exactly once", String(sum));
  check(t.structureOnly > 0, "the files with no Adobe composite are counted",
    String(t.structureOnly));
  check((wide.st.worst || []).length > 0 && wide.st.worst[0].mae > 100,
    "the worst offender is on the board",
    JSON.stringify((wide.st.worst || [])[0] || {}).slice(0, 80));
  const comparable = t.exact + t.within1 + t.within3 + t.within10 + t.over;
  check(new RegExp("of " + comparable + " comparable").test(wide.card.replace(/\s+/g, " ")),
    "the summary headline counts the comparable files",
    wide.card.split("\n").slice(0, 3).join(" | "));
  check(/Over 10\s+\d+/.test(wide.card.replace(/\s+/g, " ")) && t.over > 0,
    "the files we miss badly are in the summary, not hidden", "over=" + t.over);

  await session("light", { width: 430, height: 940 }, "narrow-light", 14);

  await browser.close();
  check(off.length === 0, "nothing is fetched off-origin", off.slice(0, 5).join(", "));

  console.log("\nThe Photoshop gauntlet — against the real server at " + base + "\n");
  console.log(notes.join("\n"));
  console.log("\ntally: " + JSON.stringify(wide.st.tally));
  console.log("total " + wide.st.total + ", comparable " + comparable +
    ", megapixels on the card " + mpNum);
  console.log("screenshots: " + shots.join(", "));
  console.log(failures === 0 ? "\nALL LIVE CHECKS PASSED\n" : "\n" + failures + " CHECK(S) FAILED\n");
  process.exit(failures === 0 ? 0 : 1);
}

(LIVE ? live(LIVE.replace(/\/$/, "")) : run()).catch((e) => {
  console.log("\nThe Photoshop gauntlet — replay harness (aborted)\n");
  console.log(notes.join("\n"));
  console.error("\n" + (e && e.message ? e.message : e));
  process.exit(1);
});
