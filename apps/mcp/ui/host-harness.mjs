/**
 * host-harness.mjs — a fake MCP App host for apps/mcp/ui/viewer.html.
 *
 * There is no way to run Claude or VS Code from a test, so this stands one up:
 * a page that embeds the viewer in a sandboxed iframe under the MCP Apps
 * default CSP and speaks the postMessage JSON-RPC of the MCP Apps extension
 * (spec 2026-01-26) back to it.
 *
 *   host -> view   answers ui/initialize, pushes ui/notifications/tool-input
 *                  and ui/notifications/tool-result with realistic
 *                  structuredContent, answers ui/request-display-mode
 *   view -> host   tools/call, notifications/message,
 *                  ui/notifications/initialized / size-changed,
 *                  ui/update-model-context — all logged
 *
 * Every pixel it sends is generated inside the page with OffscreenCanvas, so
 * the harness depends on no file in the repo but viewer.html itself.
 *
 * Run:  node apps/mcp/ui/host-harness.mjs
 * Out:  apps/mcp/ui/shots/*.png  + a pass/fail summary on stdout
 */

import { readFileSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const HERE = dirname(fileURLToPath(import.meta.url));
const SHOTS = join(HERE, "shots");
const VIEWER = join(HERE, "viewer.html");

// Playwright lives in the web app's node_modules; this folder has no package.
const require = createRequire(join(HERE, "../../web/package.json"));
const { chromium } = require("playwright");

/* ------------------------------------------------------------------ CSP */
// Verbatim from the MCP Apps spec's default policy.
const DEFAULT_CSP =
  "default-src 'none'; " +
  "script-src 'self' 'unsafe-inline'; " +
  "style-src 'self' 'unsafe-inline'; " +
  "img-src 'self' data:; " +
  "media-src 'self' data:; " +
  "connect-src 'none';";

const CSP_PROBE =
  `<meta http-equiv="Content-Security-Policy" content="${DEFAULT_CSP}">` +
  `<script>window.addEventListener("securitypolicyviolation",function(e){` +
  `parent.postMessage({__csp:{directive:e.violatedDirective,blocked:e.blockedURI}},"*")});` +
  `window.addEventListener("error",function(e){` +
  `parent.postMessage({__err:String(e.message)+" @"+e.lineno},"*")});</script>`;

function viewerWithCsp() {
  const html = readFileSync(VIEWER, "utf8");
  const i = html.indexOf("<head>");
  if (i < 0) throw new Error("viewer.html has no <head>");
  return html.slice(0, i + 6) + CSP_PROBE + html.slice(i + 6);
}

/* ------------------------------------------------------- the host page */

const HOST_PAGE = `<!doctype html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;padding:0;height:100%;background:#808080}
  iframe{display:block;width:100vw;height:100vh;border:0}
</style></head>
<body>
<iframe id="app" sandbox="allow-scripts" title="MCP App"></iframe>
<script>
(function(){
  var frame = document.getElementById("app");
  var H = window.__host = {
    log: [],            // everything the view sent
    csp: [],            // CSP violations seen inside the view
    errors: [],         // uncaught errors inside the view
    initialized: false,
    failNext: null,     // {mode:"rpc"|"isError", message:string}
    displayMode: "inline",
    session: "ph_7f3c91",
    step: 0
  };

  function send(msg){ frame.contentWindow.postMessage(msg, "*"); }
  H.send = send;
  H.notify = function(method, params){ send({jsonrpc:"2.0", method:method, params:params||{}}); };

  window.addEventListener("message", function(ev){
    var m = ev.data;
    if (m && m.__csp) { H.csp.push(m.__csp); return; }
    if (m && m.__err) { H.errors.push(m.__err); return; }
    if (!m || m.jsonrpc !== "2.0") return;
    H.log.push({ method: m.method, params: m.params, id: m.id, at: Date.now() });

    if (m.method === "ui/initialize") {
      H.initialized = true;
      send({ jsonrpc:"2.0", id:m.id, result:{
        protocolVersion: "2026-01-26",
        hostInfo: { name: "harness-host", version: "0.1.0" },
        hostCapabilities: {
          openLinks: {}, logging: {},
          serverTools: { listChanged: false },
          sandbox: { csp: {} }
        },
        hostContext: {
          theme: H.theme || undefined,
          displayMode: H.displayMode,
          availableDisplayModes: ["inline","fullscreen"],
          locale: "en-GB",
          platform: "desktop",
          deviceCapabilities: { touch:false, hover:true }
        }
      }});
      return;
    }

    if (m.method === "ui/request-display-mode") {
      H.displayMode = m.params.mode;
      send({ jsonrpc:"2.0", id:m.id, result:{ mode: H.displayMode } });
      return;
    }

    if (m.method === "ui/update-model-context" || m.method === "ui/open-link" || m.method === "ping") {
      send({ jsonrpc:"2.0", id:m.id, result:{} });
      return;
    }

    if (m.method === "tools/call") {
      H.handleToolCall(m);
      return;
    }
    // notifications need no reply
  });

  /* ---------------- pixels: generated here, never read from the repo ----- */

  function draw(w, h, edited) {
    var c = new OffscreenCanvas(w, h);
    var g = c.getContext("2d");

    // sky / sea gradient
    var sky = g.createLinearGradient(0, 0, 0, h);
    if (edited) {
      sky.addColorStop(0,   "#0d2a52"); sky.addColorStop(0.42, "#2f7fb8");
      sky.addColorStop(0.56,"#f0b26b"); sky.addColorStop(1,    "#5c3a1f");
    } else {
      sky.addColorStop(0,   "#4a5f78"); sky.addColorStop(0.42, "#7d9aab");
      sky.addColorStop(0.56,"#c9b193"); sky.addColorStop(1,    "#6d5f52");
    }
    g.fillStyle = sky; g.fillRect(0, 0, w, h);

    // sun
    var sunY = h * 0.5;
    var sun = g.createRadialGradient(w*0.68, sunY, 0, w*0.68, sunY, h*0.30);
    sun.addColorStop(0, edited ? "rgba(255,238,190,0.98)" : "rgba(240,236,225,0.72)");
    sun.addColorStop(1, "rgba(255,200,120,0)");
    g.fillStyle = sun; g.beginPath(); g.arc(w*0.68, sunY, h*0.30, 0, 6.2832); g.fill();

    // hills
    g.fillStyle = edited ? "#12202b" : "#4b5560";
    g.beginPath(); g.moveTo(0, h*0.55);
    for (var x = 0; x <= w; x += 8) {
      g.lineTo(x, h*0.55 - Math.sin(x/w*3.1)*h*0.09 - Math.sin(x/w*11)*h*0.015);
    }
    g.lineTo(w, h*0.58); g.lineTo(0, h*0.58); g.closePath(); g.fill();

    // sea glitter
    g.globalAlpha = edited ? 0.5 : 0.28;
    for (var i = 0; i < 260; i++) {
      var yy = h*0.58 + Math.random()*h*0.42;
      var ww = (1 - (yy - h*0.58)/(h*0.42)) * w * 0.05 + 3;
      g.fillStyle = edited ? "#ffd9a0" : "#d8d8d8";
      g.fillRect(w*0.68 - ww/2 + (Math.random()-0.5)*w*0.22, yy, ww, 1.6);
    }
    g.globalAlpha = 1;

    // a couple of solid shapes so edges and the layer list mean something
    g.fillStyle = edited ? "#0a1118" : "#39424b";
    g.beginPath(); g.moveTo(w*0.16, h*0.62); g.lineTo(w*0.235, h*0.30);
    g.lineTo(w*0.30, h*0.62); g.closePath(); g.fill();
    g.strokeStyle = edited ? "rgba(255,255,255,0.85)" : "rgba(255,255,255,0.5)";
    g.lineWidth = Math.max(2, w*0.004);
    g.beginPath(); g.arc(w*0.40, h*0.24, h*0.07, 0, 6.2832); g.stroke();

    // a transparent bite out of the corner, so the checkerboard is visible
    g.globalCompositeOperation = "destination-out";
    g.beginPath();
    g.moveTo(w, 0); g.lineTo(w, h*0.16); g.lineTo(w*0.84, 0); g.closePath(); g.fill();
    g.globalCompositeOperation = "source-over";

    return c;
  }

  function toDataUri(canvas) {
    return canvas.convertToBlob({ type: "image/png" }).then(function(blob){
      return new Promise(function(res){
        var fr = new FileReader();
        fr.onload = function(){ res(fr.result); };
        fr.readAsDataURL(blob);
      });
    });
  }

  function histogramOf(canvas) {
    var g = canvas.getContext("2d");
    var d = g.getImageData(0, 0, canvas.width, canvas.height).data;
    var r = new Array(256).fill(0), gg = new Array(256).fill(0),
        b = new Array(256).fill(0), l = new Array(256).fill(0);
    for (var i = 0; i < d.length; i += 4) {
      if (d[i+3] === 0) continue;
      r[d[i]]++; gg[d[i+1]]++; b[d[i+2]]++;
      l[Math.round(0.3*d[i] + 0.59*d[i+1] + 0.11*d[i+2])]++;
    }
    return { r: r, g: gg, b: b, luma: l };
  }

  /* The layer tree exactly as editor_core::summary::layer emits it: bottom
     to top, groups carrying a children array, blend kebab-case, mask an object. */
  function layerTree() {
    var L = function(id, name, kind, extra) {
      var l = { id: id, name: name, kind: kind, visible: true, opacity: 1,
                fillOpacity: 1, blend: "normal", clip: false,
                locks: { all: false, pixels: false }, colorLabel: 0, rev: 3,
                provenance: [], effects: null, mask: null };
      if (extra) Object.keys(extra).forEach(function(k){ l[k] = extra[k]; });
      return l;
    };
    return [
      L(1, "Background", "pixel"),
      L(3, "Vignette", "shape", { opacity: 0.3, blend: "multiply" }),
      L(4, "Caption", "text", { locks: { all: true, pixels: true } }),
      L(7, "Retouch", "group", { passThrough: true, expanded: true, children: [
        L(5, "Horizon patch", "pixel", { visible: false, opacity: 0.7,
          mask: { enabled: true, linked: true, density: 1 } }),
        L(6, "Dust removal", "pixel")
      ]}),
      L(8, "Sun glow", "fill", { opacity: 0.45, blend: "screen", clip: true }),
      L(9, "Warm grade", "adjustment", { opacity: 0.82, blend: "soft-light",
        mask: { enabled: true, linked: false, density: 0.9 } })
    ];
  }

  var UNDO_LABELS = ["Open", "Auto tone", "Curves", "Warm grade"];

  /* Exactly the flat snake_case object apps/mcp/src/server.rs builds:
     Open::state(id)  +  document_data(ed, id)  +  the preview fields that
     render_preview inserts. */
  H.makePayload = function(opts) {
    opts = opts || {};
    var pw = Math.min(768, opts.previewWidth || 768), ph = Math.round(pw * 2 / 3);
    var edited = opts.edited !== false;
    var c = draw(pw, ph, edited);
    return toDataUri(c).then(function(uri){
      var undo = UNDO_LABELS.slice(0, opts.undo === undefined ? 4 : opts.undo);
      var redo = (opts.redo === undefined ? 0 : opts.redo) ? ["Warm grade"] : [];
      return {
        session_id: H.session,
        path: opts.path === null ? "" : (opts.path || "/Users/dk/Pictures/harbour-dusk.psd"),
        format: "psd",
        family: "layered",
        width: 3600,
        height: 2400,
        resolution: 300,
        color_mode: "Rgba8",
        file_bytes: 48233472,
        saved: opts.dirty === false,
        revision: opts.revision === undefined ? 4 : opts.revision,
        layers: layerTree(),
        layer_lines: ["Background [pixel]", "Vignette [shape]", "Caption [text]",
                      "Retouch [group]", "  Horizon patch [pixel] +mask (hidden)",
                      "  Dust removal [pixel]", "Sun glow [fill]", "Warm grade [adjustment] +mask"],
        selection: null,
        history: { undo: undo, redo: redo },
        warnings: [],
        document: { width: 3600, height: 2400, resolution: 300, layer_count: 8 },
        histogram: histogramOf(c),
        preview_data_uri: uri,
        preview: { width: pw, height: ph, bytes: Math.round(uri.length * 0.75),
                   scale: 3600 / pw, mime_type: "image/jpeg" }
      };
    });
  };

  H.pushResult = function(opts) {
    return H.makePayload(opts).then(function(sc){
      H.notify("ui/notifications/tool-result", {
        content: [{ type: "text", text: "Preview of " + H.session + ": " +
                    sc.preview.width + "×" + sc.preview.height + " JPEG." }],
        structuredContent: sc
      });
      return sc;
    });
  };

  H.pushInput = function(args) {
    H.notify("ui/notifications/tool-input", { arguments: args || {} });
  };

  H.handleToolCall = function(m) {
    var fail = H.failNext; H.failNext = null;
    var name = m.params.name, args = m.params.arguments || {};
    setTimeout(function(){
      if (fail && fail.mode === "rpc") {
        H.send({ jsonrpc:"2.0", id:m.id, error:{ code:-32000, message: fail.message } });
        return;
      }
      if (fail && fail.mode === "isError") {
        H.send({ jsonrpc:"2.0", id:m.id, result:{
          isError: true,
          content: [{ type:"text", text: fail.message }]
        }});
        return;
      }
      var opts = {};
      if (name === "render_preview") opts.previewWidth = Math.min(768, args.max_side || 768);
      // Undo drops the warm grade, so the pixels really do go back.
      if (name === "undo") { opts.undo = 3; opts.redo = 1; opts.revision = 3; opts.edited = false; }
      if (name === "redo") { opts.undo = 4; opts.redo = 0; opts.revision = 4; }
      if (name === "save_document") {
        // The real save_document answers with the Saved struct plus session_id,
        // and nothing to draw.
        H.send({ jsonrpc:"2.0", id:m.id, result:{
          content: [{ type:"text", text: "Wrote " + args.output_path + "." }],
          structuredContent: {
            output_path: args.output_path, width: 3600, height: 2400,
            format: "psd", bytes: 47_110_144, metadata: "kept",
            session_id: args.session_id, closed: false
          }
        }});
        return;
      }
      H.makePayload(opts).then(function(sc){
        H.send({ jsonrpc:"2.0", id:m.id, result:{
          content: [{ type:"text", text: name + " ok" }],
          structuredContent: sc
        }});
      });
    }, 40);
  };

  H.mount = function(html, theme){
    H.theme = theme;
    H.log.length = 0; H.csp.length = 0; H.errors.length = 0; H.initialized = false;
    frame.srcdoc = html;
  };
})();
</script>
</body></html>`;

/* ------------------------------------------------------------- driving */

let failures = 0;
const notes = [];
function check(ok, label, detail) {
  if (ok) { notes.push(`  ok   ${label}`); }
  else { failures++; notes.push(`  FAIL ${label}${detail ? " — " + detail : ""}`); }
}

const shot = async (page, name) => {
  await page.screenshot({ path: join(SHOTS, `${name}.png`) });
  return `${name}.png`;
};

async function waitInit(page) {
  await page.waitForFunction(() => window.__host && window.__host.initialized, null, { timeout: 8000 });
  await page.waitForFunction(
    () => document.documentElement.querySelector("#app").contentWindow !== null,
    null, { timeout: 2000 }
  ).catch(() => {});
}

async function mount(page, html, theme) {
  await page.evaluate(([h, t]) => window.__host.mount(h, t), [html, theme]);
  await waitInit(page);
  await page.waitForTimeout(150);
}

/** Drag the before/after splitter grip with real mouse moves. */
async function dragSplit(page, toFraction) {
  const grip = page.frameLocator("#app").locator("#splitter .grip");
  const box = await grip.boundingBox();
  if (!box) throw new Error("the splitter grip is not on screen");
  const start = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const vp = page.viewportSize();
  const target = vp.width * toFraction;
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) {
    await page.mouse.move(start.x + (target - start.x) * (i / 8), start.y, { steps: 2 });
  }
  await page.mouse.up();
  await page.waitForTimeout(80);
  const now = await page.frameLocator("#app").locator("#splitter").getAttribute("aria-valuenow");
  return Number(now);
}

async function run() {
  rmSync(SHOTS, { recursive: true, force: true });
  mkdirSync(SHOTS, { recursive: true });

  const viewerHtml = viewerWithCsp();
  const sizeKb = statSync(VIEWER).size / 1024;

  const browser = await chromium.launch();
  const net = [];
  const produced = [];

  for (const theme of ["light", "dark"]) {
    const ctx = await browser.newContext({ colorScheme: theme, deviceScaleFactor: 2 });
    const page = await ctx.newPage();
    page.on("request", (r) => {
      const u = r.url();
      if (!u.startsWith("data:") && u !== "about:blank" && !u.startsWith("about:srcdoc")) net.push(u);
    });
    page.on("pageerror", (e) => { check(false, `no page error (${theme})`, e.message); });

    const wide = { width: 1000, height: 800 };
    const narrow = { width: 380, height: 700 };

    for (const [sizeName, vp] of [["wide", wide], ["narrow", narrow]]) {
      await page.setViewportSize(vp);
      await page.setContent(HOST_PAGE, { waitUntil: "domcontentloaded" });
      const tag = `${sizeName}-${theme}`;

      /* 1. empty state, straight after ui/initialize */
      await mount(page, viewerHtml, theme);
      check(
        await page.evaluate(() => window.__host.log.some((e) => e.method === "ui/initialize")),
        `ui/initialize sent (${tag})`
      );
      check(
        await page.evaluate(() => window.__host.log.some((e) => e.method === "ui/notifications/initialized")),
        `ui/notifications/initialized sent (${tag})`
      );
      produced.push(await shot(page, `01-empty-${tag}`));

      /* 2. open_document lands first: one preview, nothing to compare with */
      await page.evaluate(() => window.__host.pushInput({ path: "/Users/dk/Pictures/harbour-dusk.psd" }));
      await page.waitForTimeout(60);
      await page.evaluate(() => window.__host.pushResult({ edited: false, revision: 0, undo: 1 }));
      await page.waitForTimeout(450);
      check(await page.frameLocator("#app").locator("#btn-compare").isDisabled(),
        `compare disabled on the first preview (${tag})`);
      produced.push(await shot(page, `02a-first-${tag}`));

      /* 2b. then an edit at a new revision — the previous preview becomes
         the "before", which is where the split comes from */
      await page.evaluate(() => window.__host.pushInput({ session_id: "ph_7f3c91", operations: [{ op: "adjust.curves" }] }));
      await page.waitForTimeout(60);
      await page.evaluate(() => window.__host.pushResult({}));
      await page.waitForTimeout(500);
      check(await page.frameLocator("#app").locator("#splitter").isVisible(),
        `before/after split appears after an edit (${tag})`);
      const order = await page.frameLocator("#app").locator(".layerrow .nm b").allInnerTexts();
      check(order.join("|") === "Warm grade|Sun glow|Retouch|Dust removal|Horizon patch|Caption|Vignette|Background",
        `layer tree top-first, groups above their children (${tag})`, order.join("|"));
      const statusLine = await page.frameLocator("#app").locator("#status").innerText();
      check(/3,600 × 2,400 px/.test(statusLine) && /RGBA 8-bit/.test(statusLine)
            && /46 MB/.test(statusLine) && /ph_7f3c91/.test(statusLine),
        `status line carries dimensions, colour mode, size and session (${tag})`,
        statusLine.replace(/\s+/g, " "));
      produced.push(await shot(page, `02-result-${tag}`));

      /* 2b. nothing may overflow the host's width, at 380px least of all */
      const over = await page.frameLocator("#app").locator("body").evaluate((b) => ({
        body: b.scrollWidth - b.clientWidth,
        side: (function () {
          const s = b.querySelector(".side");
          return s ? s.scrollWidth - s.clientWidth : 0;
        })(),
        acts: (function () {
          const a = b.querySelector(".actions");
          return a ? a.scrollWidth - a.clientWidth : 0;
        })()
      }));
      check(over.body <= 0 && over.side <= 0 && over.acts <= 0,
        `no horizontal overflow (${tag})`, JSON.stringify(over));

      /* 2c. the side panels must not collide when the column scrolls */
      const collide = await page.frameLocator("#app").locator("body").evaluate((b) => {
        const rs = [...b.querySelectorAll(".side .panel")].map((p) => p.getBoundingClientRect());
        for (let i = 1; i < rs.length; i++) if (rs[i].top < rs[i - 1].bottom - 1) return true;
        return false;
      });
      check(!collide, `side panels do not overlap (${tag})`);

      /* 3. the before/after split, dragged */
      const splitAt = await dragSplit(page, sizeName === "wide" ? 0.27 : 0.3);
      check(splitAt > 20 && splitAt < 40, `split dragged to ~30% (${tag})`, String(splitAt));
      produced.push(await shot(page, `03-split-${tag}`));

      /* 4. zoom — the readout toggles to 100% (it is the only zoom preset
         that survives the 380px bar), then a wheel zoom in the stage */
      const fitPct = parseInt(await page.frameLocator("#app").locator("#zoomval").innerText(), 10);
      await page.frameLocator("#app").locator("#zoomval").click();
      await page.waitForTimeout(80);
      const hundred = parseInt(await page.frameLocator("#app").locator("#zoomval").innerText(), 10);
      check(hundred === 100 && fitPct < 100, `readout toggles fit -> 100% (${tag})`, `${fitPct}% -> ${hundred}%`);
      const vpNow = page.viewportSize();
      await page.mouse.move(vpNow.width * 0.45, vpNow.height * 0.3);
      await page.mouse.wheel(0, -300);
      await page.waitForTimeout(150);
      const zoomText = await page.frameLocator("#app").locator("#zoomval").innerText();
      check(/%$/.test(zoomText.trim()) && parseInt(zoomText, 10) > 0, `zoom readout (${tag})`, zoomText);
      produced.push(await shot(page, `04-zoom-${tag}`));

      /* 5. undo reaches the host as tools/call */
      await page.frameLocator("#app").locator("#zoomval").click(); // back to fit
      await page.waitForTimeout(60);
      await page.frameLocator("#app").locator("#btn-undo").click();
      await page.waitForTimeout(400);
      const undoCall = await page.evaluate(() =>
        window.__host.log.find((e) => e.method === "tools/call" && e.params.name === "undo"));
      check(!!undoCall, `undo reached the host (${tag})`);
      check(!!(undoCall && undoCall.params.arguments && undoCall.params.arguments.session_id === "ph_7f3c91"),
        `undo carried session_id (${tag})`,
        undoCall ? JSON.stringify(undoCall.params.arguments) : "no call");
      const ctxPush = await page.evaluate(() =>
        window.__host.log.some((e) => e.method === "ui/update-model-context"));
      check(ctxPush, `ui/update-model-context pushed after undo (${tag})`);
      produced.push(await shot(page, `05-undo-${tag}`));

      /* 6. a failed call shows the error state */
      await page.evaluate(() => {
        window.__host.failNext = { mode: "rpc", message: "render_preview: the session expired" };
      });
      await page.frameLocator("#app").locator("#btn-render").click();
      await page.waitForTimeout(400);
      const errVisible = await page.frameLocator("#app").locator("#err").isVisible();
      check(errVisible, `error state visible after a failed call (${tag})`);
      const errText = errVisible ? await page.frameLocator("#app").locator("#err").innerText() : "";
      check(/session expired/.test(errText), `error text shown (${tag})`, errText.slice(0, 90));
      produced.push(await shot(page, `06-error-${tag}`));

      /* 6b. an isError CallToolResult is caught too */
      await page.frameLocator("#app").locator("#err-close").click();
      await page.evaluate(() => {
        window.__host.failNext = { mode: "isError", message: "undo: nothing left on the stack" };
      });
      await page.frameLocator("#app").locator("#btn-undo").click();
      await page.waitForTimeout(400);
      check(await page.frameLocator("#app").locator("#err").isVisible(),
        `isError result also surfaces (${tag})`);
      await page.frameLocator("#app").locator("#err-close").click();

      /* 7. save asks before overwriting */
      await page.frameLocator("#app").locator("#btn-save").click();
      await page.waitForTimeout(120);
      check(await page.frameLocator("#app").locator("#ask").isVisible(),
        `save asks before overwriting (${tag})`);
      produced.push(await shot(page, `07-save-confirm-${tag}`));
      await page.frameLocator("#app").locator("#ask-ok").click();
      await page.waitForTimeout(400);
      const saveCall = await page.evaluate(() =>
        window.__host.log.find((e) => e.method === "tools/call" && e.params.name === "save_document"));
      const sa = saveCall ? saveCall.params.arguments : {};
      check(!!saveCall && sa.overwrite === true
            && sa.output_path === "/Users/dk/Pictures/harbour-dusk.psd"
            && sa.session_id === "ph_7f3c91",
        `save_document overwrote only after the prompt (${tag})`, JSON.stringify(sa));

      /* 7b. "Save a copy" must never write over the original */
      await page.frameLocator("#app").locator("#btn-save").click();
      await page.waitForTimeout(120);
      await page.frameLocator("#app").locator("#ask-copy").click();
      await page.waitForTimeout(400);
      const copyCall = await page.evaluate(() =>
        window.__host.log.filter((e) => e.method === "tools/call" && e.params.name === "save_document").pop());
      const ca = copyCall ? copyCall.params.arguments : {};
      check(ca.overwrite === false && ca.output_path === "/Users/dk/Pictures/harbour-dusk-edited.psd",
        `save a copy writes beside the original (${tag})`, JSON.stringify(ca));

      /* 8. no CSP violations and nothing uncaught */
      const csp = await page.evaluate(() => window.__host.csp);
      check(csp.length === 0, `no CSP violation (${tag})`, JSON.stringify(csp));
      const errs = await page.evaluate(() => window.__host.errors);
      check(errs.length === 0, `no uncaught error inside the view (${tag})`, JSON.stringify(errs));

      /* 9. the save receipt carries no pixels: the view must survive it,
         clear the unsaved dot and adopt the new path */
      check(await page.frameLocator("#app").locator("#stage img#img-after").isVisible(),
        `the preview survives a save receipt (${tag})`);
      check(await page.frameLocator("#app").locator("#unsaved").isHidden(),
        `the unsaved marker clears after saving (${tag})`);
      const docName = await page.frameLocator("#app").locator("#docname").innerText();
      check(docName.trim() === "harbour-dusk-edited.psd",
        `the title follows the saved copy (${tag})`, docName);

      /* 9b. render_preview must respect engine::MAX_PREVIEW_SIDE */
      const rp = await page.evaluate(() =>
        window.__host.log.filter((e) => e.method === "tools/call" && e.params.name === "render_preview").pop());
      check(!!rp && rp.params.arguments.max_side <= 768
            && rp.params.arguments.session_or_path === "ph_7f3c91",
        `render_preview stays within the 768px cap (${tag})`,
        rp ? JSON.stringify(rp.params.arguments) : "no call");
      produced.push(await shot(page, `08-after-save-${tag}`));
    }

    await ctx.close();
  }

  await browser.close();

  check(net.length === 0, "zero non-data network requests", net.join(", "));
  check(sizeKb < 100, `viewer.html under 100 KB`, `${sizeKb.toFixed(1)} KB`);

  console.log("\nMCP App viewer — host harness\n");
  console.log(notes.join("\n"));
  console.log(`\nviewer.html: ${sizeKb.toFixed(1)} KB`);
  console.log(`network requests outside data:  ${net.length}`);
  console.log(`screenshots: ${produced.length} in ${SHOTS}`);
  console.log(failures === 0 ? "\nALL CHECKS PASSED\n" : `\n${failures} CHECK(S) FAILED\n`);
  process.exit(failures === 0 ? 0 : 1);
}

run().catch((e) => {
  console.log("\nMCP App viewer — host harness (aborted)\n");
  console.log(notes.join("\n"));
  console.error("\n" + (e && e.message ? e.message : e));
  process.exit(1);
});
