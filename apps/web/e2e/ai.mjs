// AI features end to end in a real browser, on real photos.
//
//   npx vite --port 5215 --strictPort        (from apps/web, after build-wasm and fetch-models)
//   node e2e/ai.mjs [feature ...]            (default: all)
//
// Drives lib/ai.ts through the app's own editor singleton, saves every
// result to target/ai/out/<feature>-*.png plus timings.json, and fails on an
// exception. Headless Chromium has no WebGPU adapter, so this exercises the
// CPU (wasm) path; the WebGPU path needs a headed browser.

import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");
const OUT = path.join(root, process.env.GPU ? "target/ai/out-gpu" : "target/ai/out");
const IN = path.join(root, "target/ai/in");
const PHOTOS = path.join(root, "testdata/photos");
const URL_ = process.env.URL ?? "http://localhost:5215/";
fs.mkdirSync(OUT, { recursive: true });

// GPU=1: Chromium's WebGPU (Metal on this Mac) is off in headless mode by
// default; this flag turns it on so the WebGPU path and its canary run too.
const GPU = !!process.env.GPU;
const browser = await chromium.launch({ headless: process.env.HEADED ? false : true, args: GPU ? ["--enable-unsafe-webgpu"] : [] });
const timings = fs.existsSync(path.join(OUT, "timings.json")) ? JSON.parse(fs.readFileSync(path.join(OUT, "timings.json"), "utf8")) : {};
let failures = 0;

async function session() {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("console", (m) => {
    if (m.type() === "error" || m.type() === "warning") console.log(`  [${m.type()}] ${m.text().slice(0, 300)}`);
  });
  page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message}`));
  // A bare page on the dev server's origin: the engine, lib/ai.ts and io.ts
  // without the shells, so UI work in progress elsewhere cannot reload or
  // break this run. Same COOP/COEP headers as the app (threads need them).
  const harness = new URL("__ai-harness.html", URL_).href;
  await page.route(harness, (route) =>
    route.fulfill({ status: 200, contentType: "text/html", headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" }, body: "<!doctype html><title>AI harness</title>" }),
  );
  // lib/io.ts and lib/ai.ts import the editor store as Vite rewrote it
  // (with an HMR `?t=` stamp once the file has changed during this dev
  // server's life); import that exact URL or we get a second, uninitialised
  // store.
  await page.addInitScript(() => {
    window.__editorModule = async () => {
      const src = await (await fetch("/src/lib/ai.ts")).text();
      const url = src.match(/from\s+"([^"]*editor\.svelte\.ts[^"]*)"/)?.[1] ?? "/src/lib/editor.svelte.ts";
      return import(url);
    };
  });
  await page.goto(harness);
  await page.evaluate(async () => {
    const { editor } = await window.__editorModule();
    await editor.init();
  });
  return page;
}

async function open(page, file) {
  const bytes = fs.readFileSync(file).toString("base64");
  const name = path.basename(file);
  await page.evaluate(
    async ({ bytes, name }) => {
      const { openFile } = await import("/src/lib/io.ts");
      const bin = Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0));
      const type = name.endsWith(".png") ? "image/png" : "image/jpeg";
      if (!(await openFile(new File([bin], name, { type })))) throw new Error(`could not open ${name}`);
    },
    { bytes, name },
  );
}

/** Save the flattened document (or a selection overlay) as PNG. */
async function save(page, name, what = "doc") {
  const b64 = await page.evaluate(async (what) => {
    const { editor } = await window.__editorModule();
    const s = editor.summary;
    const w = s.width, h = s.height;
    let rgba = await editor.engine.call("flatten");
    if (what === "selection") {
      const sel = await editor.engine.call("selection_region", 0, 0, w, h);
      const out = new Uint8ClampedArray(w * h * 4);
      for (let i = 0; i < w * h; i++) {
        const a = sel[i] / 255;
        // Selected: the photo; unselected: dimmed and tinted red.
        out[i * 4] = rgba[i * 4] * a + (rgba[i * 4] * 0.35 + 150) * (1 - a);
        out[i * 4 + 1] = rgba[i * 4 + 1] * a + rgba[i * 4 + 1] * 0.35 * (1 - a);
        out[i * 4 + 2] = rgba[i * 4 + 2] * a + rgba[i * 4 + 2] * 0.35 * (1 - a);
        out[i * 4 + 3] = 255;
      }
      rgba = out;
    } else if (what === "checker") {
      const out = new Uint8ClampedArray(w * h * 4);
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) {
          const i = y * w + x;
          const bg = ((x >> 5) + (y >> 5)) % 2 ? 200 : 255;
          const a = rgba[i * 4 + 3] / 255;
          for (let c = 0; c < 3; c++) out[i * 4 + c] = rgba[i * 4 + c] * a + bg * (1 - a);
          out[i * 4 + 3] = 255;
        }
      rgba = out;
    }
    const c = new OffscreenCanvas(w, h);
    c.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer ?? rgba, 0, w * h * 4), w, h), 0, 0);
    const blob = await c.convertToBlob({ type: "image/png" });
    const buf = new Uint8Array(await blob.arrayBuffer());
    let s2 = "";
    for (let i = 0; i < buf.length; i += 0x8000) s2 += String.fromCharCode(...buf.subarray(i, i + 0x8000));
    return btoa(s2);
  }, what);
  fs.writeFileSync(path.join(OUT, `${name}.png`), Buffer.from(b64, "base64"));
}

async function ai(page, fn, ...args) {
  return page.evaluate(
    async ({ fn, args }) => {
      const mod = await import("/src/lib/ai.ts");
      const t0 = performance.now();
      const r = await mod[fn](...args);
      return { r, ms: Math.round(performance.now() - t0) };
    },
    { fn, args },
  );
}

async function exec(page, cmd) {
  return page.evaluate(async (cmd) => {
    const { editor } = await window.__editorModule();
    return editor.engine.exec(cmd);
  }, cmd);
}

async function summary(page) {
  return page.evaluate(async () => (await window.__editorModule()).editor.summary);
}

const FEATURES = {
  async canary(page) {
    // One model at a time, so a slow WebGPU shader compile shows up as a
    // timing rather than a silent hang.
    const ids = ["migan", "modnet", "u2netp", "edgetam-encoder", "yunet", "realesr-x4v3", "realesr-x4v3-dn50", "realesr-x4v3-wdn", "deoldify", "gfpgan", "realesrgan-x4plus", "lama"];
    const backend = GPU ? "webgpu" : "wasm";
    const r = {};
    for (const id of ids) {
      const one = await Promise.race([
        page.evaluate(async ({ backend, id }) => (await import("/src/lib/ai.ts")).runCanaries(backend, [id]), { backend, id }),
        new Promise((res) => setTimeout(() => res({ [id]: { ok: false, note: "timed out after 240 s" } }), 240_000)),
      ]);
      Object.assign(r, one);
      const v = r[id];
      console.log(`  ${id}: ok=${v.ok} dev=${v.deviation ?? "-"} backend=${v.backend ?? "-"} load=${v.loadMs}ms run=${v.runMs}ms ${v.note ?? ""}`);
      if (v.note?.startsWith("timed out")) break;
    }
    fs.writeFileSync(path.join(OUT, `canary-${backend}.json`), JSON.stringify(r, null, 1));
    return Object.fromEntries(Object.entries(r).map(([k, v]) => [k, { ok: v.ok, deviation: v.deviation, backend: v.backend, loadMs: v.loadMs, runMs: v.runMs, note: v.note }]));
  },

  async subject(page) {
    const out = {};
    for (const photo of ["portrait.jpg", "product.jpg", "street.jpg"]) {
      await open(page, path.join(PHOTOS, photo));
      const first = await page.evaluate(async () => {
        const { editor } = await window.__editorModule();
        const t0 = performance.now();
        const r = (await editor.engine.job("ai.select-subject", {}).promise).result;
        return { r, ms: Math.round(performance.now() - t0) };
      });
      const again = await ai(page, "selectBackground");
      await save(page, `subject-${path.parse(photo).name}-background`, "selection");
      await ai(page, "selectSubject");
      await save(page, `subject-${path.parse(photo).name}`, "selection");
      out[photo] = { selectSubjectMs: first.ms, selectBackgroundCachedMs: again.ms, detail: first.r };
    }
    return out;
  },

  async removebg(page) {
    await open(page, path.join(PHOTOS, "portrait.jpg"));
    const r = await ai(page, "removeBackground");
    await save(page, "removebg-portrait", "checker");
    await open(page, path.join(PHOTOS, "product.jpg"));
    const p = await ai(page, "removeBackground");
    await save(page, "removebg-product", "checker");
    return { portraitMs: r.ms, productMs: p.ms };
  },

  async blurbg(page) {
    await open(page, path.join(PHOTOS, "portrait.jpg"));
    const r = await ai(page, "blurBackground", 12);
    await save(page, "blurbg-portrait");
    return { portraitMs: r.ms };
  },

  async object(page) {
    await open(page, path.join(PHOTOS, "street.jpg"));
    const first = await ai(page, "selectObjectAt", [{ x: 700, y: 800, positive: true }]);
    await save(page, "object-street-woman", "selection");
    const second = await ai(page, "selectObjectAt", [{ x: 1810, y: 480, positive: true }]);
    await save(page, "object-street-orange-jacket", "selection");
    const third = await ai(page, "selectObjectAt", [{ x: 1810, y: 480, positive: true }, { x: 1810, y: 196, positive: false }]);
    await save(page, "object-street-jacket-minus-head", "selection");
    await open(page, path.join(PHOTOS, "product.jpg"));
    const shoe = await ai(page, "selectObjectAt", [{ x: 700, y: 850, positive: true }]);
    await save(page, "object-product-left-shoe", "selection");
    return { firstClickMs: first.ms, secondClickMs: second.ms, twoPointsMs: third.ms, productFirstClickMs: shoe.ms };
  },

  async remove(page) {
    const out = {};
    for (const quality of ["fast", "best", "auto"]) {
      // Street: select the orange-jacket man by click, then remove him.
      await open(page, path.join(PHOTOS, "street.jpg"));
      await ai(page, "selectObjectAt", [{ x: 1810, y: 480, positive: true }]);
      const r = await ai(page, "removeSelected", { quality });
      await exec(page, { op: "select.none" });
      await save(page, `remove-street-${quality}`);
      // Portrait: the lapel pin, an ellipse selection.
      await open(page, path.join(PHOTOS, "portrait.jpg"));
      await exec(page, { op: "select.ellipse", x: 1470, y: 1490, width: 140, height: 90 });
      const pin = await ai(page, "removeSelected", { quality });
      await exec(page, { op: "select.none" });
      await save(page, `remove-portrait-pin-${quality}`);
      out[quality] = { streetPersonMs: r.ms, lapelPinMs: pin.ms };
    }
    return out;
  },

  async upscale(page) {
    await open(page, path.join(IN, "small-face.png"));
    const s0 = await summary(page);
    await save(page, "upscale-input");
    const fast = await ai(page, "upscale", 4, { quality: "fast" });
    await save(page, "upscale-4x-fast");
    await open(page, path.join(IN, "small-face.png"));
    const two = await ai(page, "upscale", 2, { quality: "fast" });
    await save(page, "upscale-2x-fast");
    await open(page, path.join(IN, "small-face.png"));
    const best = await ai(page, "upscale", 4, { quality: "best" });
    const s1 = await summary(page);
    await save(page, "upscale-4x-best");
    return { input: `${s0.width}x${s0.height}`, output4x: `${s1.width}x${s1.height}`, fast4xMs: fast.ms, fast2xMs: two.ms, best4xMs: best.ms };
  },

  async enhance(page) {
    await open(page, path.join(IN, "noisy.png"));
    const d = await ai(page, "denoise", "medium");
    await save(page, "denoise-medium");
    await open(page, path.join(IN, "noisy.png"));
    const dh = await ai(page, "denoise", "high");
    await save(page, "denoise-high");
    await open(page, path.join(IN, "jpeg-q12.jpg"));
    const j = await ai(page, "removeJpegArtifacts");
    await save(page, "jpeg-clean");
    const s = await summary(page);
    return { size: `${s.width}x${s.height}`, denoiseMediumMs: d.ms, denoiseHighMs: dh.ms, jpegMs: j.ms };
  },

  async faces(page) {
    await open(page, path.join(IN, "old-bw-small.jpg"));
    const det = await ai(page, "detectFaces");
    const r = await ai(page, "restoreFaces");
    await save(page, "faces-restored");
    await open(page, path.join(PHOTOS, "old-bw.jpg"));
    const c = await ai(page, "colorize");
    await save(page, "colorized");
    return { detected: det.r.length, detectMs: det.ms, restoreMs: r.ms, faces: r.r, colorizeMs: c.ms };
  },

  async scene(page) {
    const out = {};
    for (const photo of ["portrait.jpg", "street.jpg", "landscape.jpg", "product.jpg", "old-bw.jpg"]) {
      await open(page, path.join(PHOTOS, photo));
      const f = await ai(page, "detectFaces");
      const s = await ai(page, "analyzeScene");
      out[photo] = { faces: f.r.length, detectMs: f.ms, scene: s.r, analyzeMs: s.ms };
      console.log(`  ${photo}: ${JSON.stringify(s.r)}`);
    }
    return out;
  },

  async plan(page) {
    await open(page, path.join(PHOTOS, "landscape.jpg"));
    const out = {};
    for (const text of ["brighter and a bit warmer", "make it pop", "remove the background", "black and white with more contrast", "replace the sky with a sunset", "make the dog talk"]) {
      const r = await ai(page, "planEdit", text);
      out[text] = r.r;
      console.log(`  "${text}" → ${JSON.stringify(r.r)}`);
    }
    // Run one plan for real.
    const p = await ai(page, "planEdit", "brighter, a bit warmer and more contrast");
    await ai(page, "runPlan", p.r.steps);
    await save(page, "plan-brighter-warmer-contrast");
    return out;
  },
};

const wanted = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(FEATURES);
for (const name of wanted) {
  console.log(`● ${name}`);
  const page = await session();
  try {
    const r = await FEATURES[name](page);
    timings[name] = r;
    console.log(`  ${JSON.stringify(r).slice(0, 600)}`);
  } catch (e) {
    failures++;
    console.log(`  FAILED: ${e.message}`);
  } finally {
    await page.close();
  }
  fs.writeFileSync(path.join(OUT, "timings.json"), JSON.stringify(timings, null, 1));
}
await browser.close();
process.exit(failures ? 1 : 0);
