// End-to-end: the real `openphotoedit` binary, a real browser, a real photo.
//
//   node crates/editor-server/e2e/native.mjs [binary] [photo] [out.png]
//
// Starts `openphotoedit open <photo> --port 5216 --no-open`, then checks in
// Chromium that the page is cross-origin isolated, the wasm engine starts
// with no console errors, /api/health reports the native backend, the
// one-time file token works exactly once, and a photo opens in Lite. Ends
// with Ctrl-C (SIGINT) and expects a clean exit.

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { statSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
// Playwright is installed in apps/web, not here.
const require = createRequire(resolve(repo, "apps/web/package.json"));
const { chromium } = require("playwright");

const bin = resolve(process.argv[2] ?? `${repo}/target/server/release/openphotoedit`);
const photo = resolve(process.argv[3] ?? `${repo}/testdata/photos/portrait.jpg`);
const out = resolve(process.argv[4] ?? `${repo}/target/server/out/native-e2e.png`);
const PORT = 5216;
mkdirSync(dirname(out), { recursive: true });

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const server = spawn(bin, ["open", photo, "--port", String(PORT), "--no-open"], { stdio: ["ignore", "pipe", "pipe"] });
let stdout = "";
let stderr = "";
server.stdout.on("data", (d) => (stdout += d));
server.stderr.on("data", (d) => (stderr += d));
const exited = new Promise((res) => server.on("exit", (code, signal) => res({ code, signal })));

const openUrl = await new Promise((res, rej) => {
  const t = setTimeout(() => rej(new Error(`server did not print a URL:\n${stdout}\n${stderr}`)), 10000);
  const poll = setInterval(() => {
    const m = stdout.match(/Open: (http:\/\/\S+)/);
    if (m) {
      clearInterval(poll);
      clearTimeout(t);
      res(m[1]);
    }
  }, 50);
});
const base = `http://127.0.0.1:${PORT}/`;
const token = new URL(openUrl).searchParams.get("open");
check(openUrl.startsWith(base) && /^[0-9a-f]{64}$/.test(token ?? ""), `printed a token URL on port ${PORT}`);

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("console", (m) => {
  // The deliberate second token fetch below logs its 410; nothing else may.
  if (m.type() === "error" && !m.text().includes("status of 410")) errors.push(m.text());
});
page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
const wasmTypes = [];
page.on("response", (r) => {
  if (r.url().endsWith(".wasm")) wasmTypes.push(`${r.status()} ${r.headers()["content-type"]}`);
});

await page.goto(base);
check(await page.evaluate(() => crossOriginIsolated), "page is cross-origin isolated (COOP/COEP)");

const health = await page.evaluate(async () => (await fetch("/api/health")).json());
check(health.app === "openphotoedit" && health.native === true && health.web === "embedded", `health: ${JSON.stringify(health)}`);

const size = statSync(photo).size;
const handoff = await page.evaluate(async (t) => {
  const first = await fetch(`/api/file/${t}`);
  const bytes = (await first.arrayBuffer()).byteLength;
  const second = await fetch(`/api/file/${t}`);
  return { status: first.status, bytes, name: decodeURIComponent(first.headers.get("x-file-name") ?? ""), type: first.headers.get("content-type"), again: second.status };
}, token);
check(handoff.status === 200 && handoff.bytes === size, `token fetch returned the photo (${handoff.bytes} of ${size} bytes, ${handoff.type}, ${handoff.name})`);
check(handoff.again === 410, `second fetch of the token is refused (${handoff.again})`);

await page.getByTestId("choose-lite").click();
const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("open").click()]);
await chooser.setFiles(photo);
await page.waitForFunction(() => !document.querySelector('[data-testid="busy"]'), null, { timeout: 30000 });
await page.waitForTimeout(1200);
check(wasmTypes.length > 0 && wasmTypes.every((t) => t === "200 application/wasm"), `wasm served as application/wasm (${wasmTypes.join(", ")})`);

// The viewport canvas has the photo on it: sample it for non-uniform pixels.
const painted = await page.evaluate(() => {
  const canvases = [...document.querySelectorAll("canvas")].filter((c) => c.width > 100 && c.height > 100);
  for (const c of canvases) {
    const ctx = c.getContext("2d");
    if (!ctx) continue;
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    const seen = new Set();
    for (let i = 0; i < d.length; i += 4 * 997) seen.add((d[i] >> 4) * 256 + (d[i + 1] >> 4) * 16 + (d[i + 2] >> 4));
    if (seen.size > 50) return seen.size;
  }
  return 0;
});
check(painted > 50, `viewport shows the photo (${painted} distinct colours sampled)`);

await page.screenshot({ path: out });
check(errors.length === 0, `no console errors${errors.length ? `: ${errors.join(" | ")}` : ""}`);
await browser.close();

server.kill("SIGINT");
const result = await Promise.race([exited, new Promise((res) => setTimeout(() => res({ code: "timeout" }), 5000))]);
check(result.code === 0, `graceful shutdown on Ctrl-C (exit ${result.code}${result.signal ? `, ${result.signal}` : ""})`);
check(!/WARN|ERROR/.test(stderr), `quiet stderr${stderr.trim() ? `: ${stderr.trim()}` : ""}`);

console.log(`screenshot: ${out}`);
if (failures.length) {
  console.log(`${failures.length} check(s) failed`);
  process.exit(1);
}
