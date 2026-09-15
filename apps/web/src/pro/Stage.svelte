<script lang="ts">
  // The document area: rulers, the canvas, the Pro overlay (pixel grid,
  // guides, Quick Mask tint) and the empty state. Tracks the cursor for the
  // Info panel and the status bar.
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Canvas from "../canvas/Canvas.svelte";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { formatShortcut } from "../ui/platform";
  import { paintTarget } from "../ui/paint-target.svelte";
  import { ACTIONS } from "./actions.svelte";
  import { pro } from "./state.svelte";

  const RULER = 18;
  let area: HTMLDivElement;
  let overlay: HTMLCanvasElement;
  let topRuler: HTMLCanvasElement | undefined = $state();
  let leftRuler: HTMLCanvasElement | undefined = $state();
  let size = $state({ w: 0, h: 0 });
  let qmMask: { data: Uint8Array; x: number; y: number; scale: number; w: number; h: number } | null = null;

  const showRulers = $derived(pro.layout.rulers && editor.hasDocument);

  function tickStep(zoom: number) {
    const steps = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000, 10000, 20000, 50000];
    return steps.find((s) => s * zoom >= 60) ?? 100000;
  }

  function style(name: string, fallback: string) {
    return getComputedStyle(area).getPropertyValue(name).trim() || fallback;
  }

  function drawRuler(c: HTMLCanvasElement | undefined, axis: "x" | "y") {
    if (!c || !editor.summary) return;
    const dpr = window.devicePixelRatio || 1;
    const len = axis === "x" ? size.w : size.h;
    const W = axis === "x" ? len : RULER;
    const H = axis === "x" ? RULER : len;
    c.width = Math.max(1, Math.round(W * dpr));
    c.height = Math.max(1, Math.round(H * dpr));
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = style("--bg-page", "#111");
    g.fillRect(0, 0, W, H);
    const { zoom } = editor.view;
    const step = tickStep(zoom);
    const minor = step / (String(step).startsWith("25") ? 5 : 10);
    const d0 = axis === "x" ? editor.toDoc(0, 0).x : editor.toDoc(0, 0).y;
    const d1 = axis === "x" ? editor.toDoc(len, 0).x : editor.toDoc(0, len).y;
    g.strokeStyle = style("--text-faint", "rgba(255,255,255,.5)");
    g.fillStyle = style("--text-muted", "rgba(255,255,255,.6)");
    g.font = `10px ${style("--font-sans", "sans-serif")}`;
    g.lineWidth = 1;
    g.beginPath();
    for (let v = Math.floor(d0 / minor) * minor; v <= d1; v += minor) {
      const p = axis === "x" ? editor.toView(v, 0).x : editor.toView(0, v).y;
      const major = Math.abs(v / step - Math.round(v / step)) < 1e-6;
      const half = !major && Math.abs((v * 2) / step - Math.round((v * 2) / step)) < 1e-6;
      const tl = major ? RULER : half ? 7 : 4;
      const pp = Math.round(p) + 0.5;
      if (axis === "x") {
        g.moveTo(pp, RULER);
        g.lineTo(pp, RULER - tl);
      } else {
        g.moveTo(RULER, pp);
        g.lineTo(RULER - tl, pp);
      }
      if (major) {
        const label = String(Math.round(v));
        if (axis === "x") g.fillText(label, pp + 3, 9);
        else {
          g.save();
          g.translate(9, pp + 3);
          g.rotate(-Math.PI / 2);
          g.textAlign = "right";
          g.fillText(label, 0, 0);
          g.restore();
        }
      }
    }
    g.stroke();
    // Cursor marker.
    if (pro.cursor) {
      const p = axis === "x" ? editor.toView(pro.cursor.x, 0).x : editor.toView(0, pro.cursor.y).y;
      g.strokeStyle = style("--text-strong", "#fff");
      g.beginPath();
      if (axis === "x") {
        g.moveTo(Math.round(p) + 0.5, 0);
        g.lineTo(Math.round(p) + 0.5, RULER);
      } else {
        g.moveTo(0, Math.round(p) + 0.5);
        g.lineTo(RULER, Math.round(p) + 0.5);
      }
      g.stroke();
    }
    g.fillStyle = style("--border-hairline", "rgba(255,255,255,.1)");
    if (axis === "x") g.fillRect(0, RULER - 1, W, 1);
    else g.fillRect(RULER - 1, 0, 1, H);
  }

  function drawOverlay() {
    if (!overlay) return;
    const dpr = window.devicePixelRatio || 1;
    overlay.width = Math.max(1, Math.round(size.w * dpr));
    overlay.height = Math.max(1, Math.round(size.h * dpr));
    const g = overlay.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, size.w, size.h);
    const s = editor.summary;
    if (!s || !editor.hasDocument) return;
    const { zoom } = editor.view;
    const tl = editor.toView(0, 0);
    const br = editor.toView(s.width, s.height);

    if (paintTarget.quickMask && qmMask) {
      const m = qmMask;
      const img = new ImageData(m.w, m.h);
      for (let i = 0; i < m.w * m.h; i++) {
        img.data[i * 4] = 255;
        img.data[i * 4 + 3] = Math.round((255 - m.data[i]) * 0.5);
      }
      const tmp = new OffscreenCanvas(m.w, m.h);
      tmp.getContext("2d")!.putImageData(img, 0, 0);
      const p = editor.toView(m.x, m.y);
      g.imageSmoothingEnabled = false;
      g.drawImage(tmp, p.x, p.y, (m.w / m.scale) * zoom, (m.h / m.scale) * zoom);
    }

    if (pro.layout.extras && pro.layout.pixelGrid && zoom >= 8) {
      g.strokeStyle = "rgba(128,128,128,0.35)";
      g.lineWidth = 1;
      g.beginPath();
      const x0 = Math.max(0, Math.floor(editor.toDoc(0, 0).x));
      const x1 = Math.min(s.width, Math.ceil(editor.toDoc(size.w, 0).x));
      const y0 = Math.max(0, Math.floor(editor.toDoc(0, 0).y));
      const y1 = Math.min(s.height, Math.ceil(editor.toDoc(0, size.h).y));
      for (let x = x0; x <= x1; x++) {
        const px = Math.round(editor.toView(x, 0).x) + 0.5;
        g.moveTo(px, Math.max(0, tl.y));
        g.lineTo(px, Math.min(size.h, br.y));
      }
      for (let y = y0; y <= y1; y++) {
        const py = Math.round(editor.toView(0, y).y) + 0.5;
        g.moveTo(Math.max(0, tl.x), py);
        g.lineTo(Math.min(size.w, br.x), py);
      }
      g.stroke();
    }

    if (pro.layout.extras && pro.layout.showGuides) {
      // Photoshop's guide cyan: a data colour on the image, not UI chrome.
      g.strokeStyle = "rgb(0 220 255)";
      g.lineWidth = 1;
      g.beginPath();
      for (const gd of [...pro.guides, ...(pro.draftGuide ? [pro.draftGuide] : [])]) {
        if (gd.axis === "x") {
          const px = Math.round(editor.toView(gd.pos, 0).x) + 0.5;
          g.moveTo(px, 0);
          g.lineTo(px, size.h);
        } else {
          const py = Math.round(editor.toView(0, gd.pos).y) + 0.5;
          g.moveTo(0, py);
          g.lineTo(size.w, py);
        }
      }
      g.stroke();
    }
  }

  async function refreshQuickMask() {
    const s = editor.summary;
    if (!paintTarget.quickMask || !s || !editor.hasDocument) {
      qmMask = null;
      return;
    }
    const scale = Math.min(1, 1024 / Math.max(s.width, s.height));
    const w = Math.max(1, Math.round(s.width * scale));
    const h = Math.max(1, Math.round(s.height * scale));
    try {
      let data: Uint8Array = await editor.engine.call<Uint8Array>("selection_view", 0, 0, scale, w, h);
      if (!data.length) data = new Uint8Array(w * h).fill(0);
      qmMask = { data, x: 0, y: 0, scale, w, h };
    } catch {
      qmMask = null;
    }
    drawOverlay();
  }

  $effect(() => {
    void editor.view.cx;
    void editor.view.cy;
    void editor.view.zoom;
    void size.w;
    void size.h;
    void editor.summary?.revision;
    void pro.layout.pixelGrid;
    void pro.layout.extras;
    void pro.layout.showGuides;
    void pro.guides.length;
    void pro.draftGuide?.pos;
    void paintTarget.quickMask;
    void editor.hasDocument;
    drawOverlay();
  });

  $effect(() => {
    void editor.summary?.revision;
    void paintTarget.quickMask;
    void refreshQuickMask();
  });

  $effect(() => {
    void editor.view.cx;
    void editor.view.cy;
    void editor.view.zoom;
    void size.w;
    void size.h;
    void pro.cursor;
    void showRulers;
    if (!showRulers) return;
    drawRuler(topRuler, "x");
    drawRuler(leftRuler, "y");
  });

  let sampleTimer = 0;
  function track(e: PointerEvent) {
    if (!editor.hasDocument) return;
    const r = area.getBoundingClientRect();
    const d = editor.toDoc(e.clientX - r.left, e.clientY - r.top);
    pro.cursor = d;
    if (pro.draftGuide) return;
    if (!sampleTimer) {
      sampleTimer = window.setTimeout(async () => {
        sampleTimer = 0;
        const c = pro.cursor;
        const s = editor.summary;
        if (!c || !s || c.x < 0 || c.y < 0 || c.x >= s.width || c.y >= s.height) {
          pro.cursorColor = null;
          return;
        }
        try {
          const px = await editor.engine.call<Uint8Array>("region", Math.floor(c.x), Math.floor(c.y), 1, 1);
          pro.cursorColor = { r: px[0], g: px[1], b: px[2], a: px[3] };
        } catch {
          pro.cursorColor = null;
        }
      }, 50);
    }
  }

  function guideDown(axis: "x" | "y", e: PointerEvent) {
    if (e.button !== 0 || !editor.hasDocument) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    guideMove(axis, e);
  }
  function guideMove(axis: "x" | "y", e: PointerEvent) {
    if (e.buttons !== 1) return;
    const r = area.getBoundingClientRect();
    const d = editor.toDoc(e.clientX - r.left, e.clientY - r.top);
    // The top ruler makes horizontal guides (fixed y), the left one vertical.
    pro.draftGuide = axis === "y" ? { axis: "y", pos: Math.round(d.y) } : { axis: "x", pos: Math.round(d.x) };
  }
  function guideUp(e: PointerEvent) {
    const g = pro.draftGuide;
    pro.draftGuide = null;
    if (!g) return;
    const r = area.getBoundingClientRect();
    if (e.clientX >= r.left && e.clientY >= r.top && e.clientX <= r.right && e.clientY <= r.bottom) pro.guides = [...pro.guides, g];
  }
</script>

<div class="stage" class:rulers={showRulers}>
  {#if showRulers}
    <div class="corner" aria-hidden="true"></div>
    <canvas
      class="ruler top"
      bind:this={topRuler}
      style:height="{RULER}px"
      aria-label={t("Horizontal ruler. Drag down to add a guide.")}
      onpointerdown={(e) => guideDown("y", e)}
      onpointermove={(e) => guideMove("y", e)}
      onpointerup={guideUp}
    ></canvas>
    <canvas
      class="ruler left"
      bind:this={leftRuler}
      style:width="{RULER}px"
      aria-label={t("Vertical ruler. Drag right to add a guide.")}
      onpointerdown={(e) => guideDown("x", e)}
      onpointermove={(e) => guideMove("x", e)}
      onpointerup={guideUp}
    ></canvas>
  {/if}
  <div
    class="area"
    bind:this={area}
    bind:clientWidth={size.w}
    bind:clientHeight={size.h}
    role="presentation"
    onpointermove={track}
    onpointerleave={() => {
      pro.cursor = null;
      pro.cursorColor = null;
    }}
  >
    <Canvas />
    <canvas class="overlay" bind:this={overlay} style:width="{size.w}px" style:height="{size.h}px" aria-hidden="true"></canvas>
    {#if !editor.hasDocument}
      <div class="empty" data-testid="empty-state">
        <h2>{t("Start with an image")}</h2>
        <p>{t("Open a photo or a PSD, or make a blank document. Everything stays on this device.")}</p>
        <div class="actions">
          <button type="button" class="oa-btn oa-btn--primary oa-btn--md" data-testid="empty-open" onclick={() => ACTIONS["file.open"].run()}>
            <FolderOpen size={15} />
            {t("Open…")}
            <kbd>{formatShortcut("Mod+O")}</kbd>
          </button>
          <button type="button" class="oa-btn oa-btn--secondary oa-btn--md" data-testid="empty-new" onclick={() => pro.open({ kind: "new" })}>
            <FilePlus size={15} />
            {t("New…")}
            <kbd>{formatShortcut("Mod+N")}</kbd>
          </button>
        </div>
        <p class="drop">{t("Or drop a file anywhere in the window.")}</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    overflow: hidden;
    background: var(--ops-pasteboard);
  }
  .stage.rulers {
    grid-template-columns: 18px minmax(0, 1fr);
    grid-template-rows: 18px minmax(0, 1fr);
  }
  .corner {
    background: var(--bg-page);
    border-right: var(--border-width) solid var(--border-hairline);
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .ruler {
    display: block;
    width: 100%;
    height: 100%;
    cursor: default;
    touch-action: none;
  }
  .ruler.top {
    cursor: row-resize;
  }
  .ruler.left {
    cursor: col-resize;
  }
  .area {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .stage.rulers .area {
    grid-column: 2;
    grid-row: 2;
  }
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-6);
    text-align: center;
    background: var(--bg-subtle);
  }
  h2 {
    margin: 0;
    font: var(--type-h3);
    letter-spacing: var(--tracking-heading);
    color: var(--text-strong);
  }
  p {
    margin: 0;
    max-width: 420px;
    font: var(--weight-regular) var(--text-sm) / 1.5 var(--font-sans);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
  kbd {
    font: var(--weight-regular) var(--text-2xs) / 1 var(--font-sans);
    opacity: 0.6;
    margin-left: 4px;
  }
  .drop {
    margin-top: var(--space-2);
    font: var(--type-caption);
    color: var(--text-faint);
  }
</style>
