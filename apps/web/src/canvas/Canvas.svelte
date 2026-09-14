<script lang="ts">
  // The document viewport: renders the composite through the engine, draws
  // selection outlines and tool overlays, and turns pointer input into tool
  // calls in document coordinates.
  import { onMount } from "svelte";
  import { editor } from "../lib/editor.svelte";
  import { TOOLS } from "../tools/registry";
  import type { ToolPointer } from "../tools/types";

  let { toolOverride = null }: { toolOverride?: string | null } = $props();

  let host: HTMLDivElement;
  let base: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let dpr = window.devicePixelRatio || 1;

  // Last rendered frame, kept so a pan or zoom redraws instantly while the
  // engine renders the new view.
  let frame: { bitmap: ImageBitmap; x: number; y: number; scale: number } | null = null;
  let inFlight = false;
  let pending = false;
  let spaceHeld = $state(false);
  let pressed = false;
  let pointerDownTool: string | null = null;

  // Selection outline: edge pixels in device space for the current frame.
  let edges: Int32Array | null = null;
  let edgeFrame: { x: number; y: number; scale: number; w: number; h: number; i0: number; j0: number } | null = null;
  let antsPhase = 0;
  let antsImg: ImageData | null = null;
  let antsWritten: number[] = [];

  const activeToolId = $derived(spaceHeld ? "hand" : (toolOverride ?? editor.tool));
  const cursor = $derived(TOOLS[activeToolId]?.cursor ?? "default");

  function checker(ctx: CanvasRenderingContext2D) {
    const size = Math.max(4, Math.round(8 * dpr));
    const c = new OffscreenCanvas(size * 2, size * 2);
    const g = c.getContext("2d")!;
    const dark = getComputedStyle(host).getPropertyValue("--ops-checker-dark").trim() || "#d9d9d9";
    const light = getComputedStyle(host).getPropertyValue("--ops-checker-light").trim() || "#ffffff";
    g.fillStyle = light;
    g.fillRect(0, 0, size * 2, size * 2);
    g.fillStyle = dark;
    g.fillRect(0, 0, size, size);
    g.fillRect(size, size, size, size);
    return ctx.createPattern(c, "repeat")!;
  }

  /** Device-pixel geometry of the current view. `quality` < 1 renders
   *  fewer pixels (drawn scaled up) while the user is dragging. */
  function geometry(quality = 1) {
    const s = editor.summary!;
    const W = Math.round(editor.viewport.width * dpr * quality);
    const H = Math.round(editor.viewport.height * dpr * quality);
    const scale = editor.view.zoom * dpr * quality;
    const x0 = editor.view.cx - W / 2 / scale;
    const y0 = editor.view.cy - H / 2 / scale;
    const i0 = Math.max(0, Math.floor((0 - x0) * scale));
    const j0 = Math.max(0, Math.floor((0 - y0) * scale));
    const i1 = Math.min(W, Math.ceil((s.width - x0) * scale));
    const j1 = Math.min(H, Math.ceil((s.height - y0) * scale));
    return { W, H, scale, x0, y0, i0, j0, w: Math.max(0, i1 - i0), h: Math.max(0, j1 - j0) };
  }

  function drawFrame() {
    const ctx = base.getContext("2d")!;
    const s = editor.summary;
    ctx.clearRect(0, 0, base.width, base.height);
    if (!s || !editor.hasDocument) return;
    const g = geometry();
    // Checkerboard behind the document's area only.
    const left = (0 - g.x0) * g.scale;
    const top = (0 - g.y0) * g.scale;
    ctx.save();
    ctx.fillStyle = checker(ctx);
    ctx.translate(Math.round(left), Math.round(top));
    ctx.fillRect(0, 0, Math.round(s.width * g.scale), Math.round(s.height * g.scale));
    ctx.restore();
    if (frame) {
      const k = g.scale / frame.scale;
      const dx = (frame.x - g.x0) * g.scale;
      const dy = (frame.y - g.y0) * g.scale;
      ctx.imageSmoothingEnabled = k < 1;
      ctx.drawImage(frame.bitmap, dx, dy, frame.bitmap.width * k, frame.bitmap.height * k);
    }
  }

  let pendingQuality = 1;

  async function renderNow(quality = 1) {
    if (!editor.summary || !editor.hasDocument) {
      drawFrame();
      return;
    }
    if (inFlight) {
      pending = true;
      pendingQuality = quality;
      return;
    }
    inFlight = true;
    try {
      do {
        pending = false;
        const g = geometry(quality);
        quality = pendingQuality;
        pendingQuality = 1;
        if (g.w === 0 || g.h === 0) {
          frame = null;
          drawFrame();
          break;
        }
        const rx = g.x0 + g.i0 / g.scale;
        const ry = g.y0 + g.j0 / g.scale;
        const data = await editor.engine.render(rx, ry, g.scale, g.w, g.h);
        const bitmap = await createImageBitmap(new ImageData(data, g.w, g.h), { premultiplyAlpha: "premultiply" });
        frame?.bitmap.close();
        frame = { bitmap, x: rx, y: ry, scale: g.scale };
        drawFrame();
        if (editor.summary?.selection) {
          const sel = await editor.engine.call<Uint8Array>("selection_view", rx, ry, g.scale, g.w, g.h);
          edges = sel.length ? findEdges(sel, g.w, g.h) : null;
          edgeFrame = { x: rx, y: ry, scale: g.scale, w: g.w, h: g.h, i0: g.i0, j0: g.j0 };
        } else {
          edges = null;
        }
        drawOverlay();
      } while (pending);
    } catch (e) {
      console.error("render failed", e);
    } finally {
      inFlight = false;
    }
  }

  function findEdges(sel: Uint8Array, w: number, h: number): Int32Array {
    const out: number[] = [];
    const inside = (x: number, y: number) => x >= 0 && y >= 0 && x < w && y < h && sel[y * w + x] >= 128;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        if (!inside(x, y)) continue;
        if (!inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1)) out.push(x, y);
      }
    }
    return Int32Array.from(out);
  }

  function drawOverlay() {
    if (!overlay) return;
    const ctx = overlay.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, overlay.width, overlay.height);
    if (!editor.hasDocument) return;
    const g = geometry();
    if (edges && edgeFrame && Math.abs(edgeFrame.scale - g.scale) < 1e-9) {
      // Marching ants: alternate black and white by diagonal position.
      const ox = Math.round((edgeFrame.x - g.x0) * g.scale);
      const oy = Math.round((edgeFrame.y - g.y0) * g.scale);
      if (!antsImg || antsImg.width !== overlay.width || antsImg.height !== overlay.height) {
        antsImg = ctx.createImageData(overlay.width, overlay.height);
        antsWritten = [];
      }
      const img = antsImg;
      const d = img.data;
      // Clear only what the previous pass wrote; a full clear of a 4K
      // buffer eight times a second is needless.
      for (const o of antsWritten) d[o + 3] = 0;
      antsWritten = [];
      for (let k = 0; k < edges.length; k += 2) {
        const x = edges[k] + ox;
        const y = edges[k + 1] + oy;
        if (x < 0 || y < 0 || x >= overlay.width || y >= overlay.height) continue;
        const on = ((x + y + antsPhase) & 8) === 0;
        const o = (y * overlay.width + x) * 4;
        const v = on ? 0 : 255;
        d[o] = v;
        d[o + 1] = v;
        d[o + 2] = v;
        d[o + 3] = 255;
        antsWritten.push(o);
      }
      ctx.putImageData(img, 0, 0);
    }
    const tool = TOOLS[activeToolId];
    if (tool?.overlay) {
      ctx.save();
      ctx.scale(dpr, dpr);
      tool.overlay(editor, ctx);
      ctx.restore();
    }
  }

  function pointer(e: PointerEvent | WheelEvent): ToolPointer {
    const r = overlay.getBoundingClientRect();
    const vx = e.clientX - r.left;
    const vy = e.clientY - r.top;
    const d = editor.toDoc(vx, vy);
    const isMac = navigator.platform.toLowerCase().includes("mac");
    return {
      x: d.x,
      y: d.y,
      vx,
      vy,
      pressure: "pressure" in e && e.pointerType === "pen" ? e.pressure : 1,
      shift: e.shiftKey,
      alt: e.altKey,
      mod: isMac ? e.metaKey : e.ctrlKey,
      button: e.button,
      pointerType: "pointerType" in e ? e.pointerType : "mouse",
    };
  }

  // Two-finger touch: pinch zoom and pan.
  const touches = new Map<number, { x: number; y: number }>();
  let pinch: { dist: number; mx: number; my: number } | null = null;

  function onPointerDown(e: PointerEvent) {
    if (!editor.hasDocument) return;
    overlay.setPointerCapture(e.pointerId);
    if (e.pointerType === "touch") {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size === 2) {
        if (pressed && pointerDownTool) TOOLS[pointerDownTool]?.cancel?.(editor);
        pressed = false;
        const [a, b] = [...touches.values()];
        pinch = { dist: Math.hypot(a.x - b.x, a.y - b.y), mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2 };
        return;
      }
    }
    if (e.button === 1) {
      pointerDownTool = "hand";
    } else {
      pointerDownTool = activeToolId;
    }
    pressed = true;
    TOOLS[pointerDownTool]?.down?.(editor, pointer(e));
    drawOverlay();
  }

  function onPointerMove(e: PointerEvent) {
    if (!editor.hasDocument) return;
    if (e.pointerType === "touch" && touches.has(e.pointerId)) {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pinch && touches.size === 2) {
        const [a, b] = [...touches.values()];
        const dist = Math.hypot(a.x - b.x, a.y - b.y);
        const mx = (a.x + b.x) / 2;
        const my = (a.y + b.y) / 2;
        const r = overlay.getBoundingClientRect();
        editor.panBy(mx - pinch.mx, my - pinch.my);
        editor.zoomAt(editor.view.zoom * (dist / pinch.dist), mx - r.left, my - r.top);
        pinch = { dist, mx, my };
        return;
      }
    }
    const events = pressed && "getCoalescedEvents" in e ? e.getCoalescedEvents() : [e];
    const tool = TOOLS[pressed ? pointerDownTool ?? activeToolId : activeToolId];
    for (const ev of events.length ? events : [e]) tool?.move?.(editor, pointer(ev), pressed);
    drawOverlay();
  }

  function onPointerUp(e: PointerEvent) {
    touches.delete(e.pointerId);
    if (pinch) {
      if (touches.size < 2) pinch = null;
      return;
    }
    if (!pressed) return;
    pressed = false;
    const id = pointerDownTool ?? activeToolId;
    pointerDownTool = null;
    TOOLS[id]?.up?.(editor, pointer(e));
    drawOverlay();
  }

  function onWheel(e: WheelEvent) {
    if (!editor.hasDocument) return;
    e.preventDefault();
    const p = pointer(e);
    if (e.ctrlKey || e.metaKey) {
      // Trackpad pinch arrives as ctrl+wheel with small deltas.
      const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0025));
      editor.zoomAt(editor.view.zoom * factor, p.vx, p.vy);
    } else if (e.altKey) {
      editor.zoomStep(e.deltaY < 0 ? 1 : -1, p.vx, p.vy);
    } else {
      const k = e.deltaMode === 1 ? 16 : 1;
      editor.panBy(-(e.shiftKey ? e.deltaY : e.deltaX) * k, -(e.shiftKey ? 0 : e.deltaY) * k);
    }
  }

  function isTyping(t: EventTarget | null) {
    const el = t as HTMLElement | null;
    return !!el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
  }

  function onKeyDown(e: KeyboardEvent) {
    if (isTyping(e.target)) return;
    if (e.code === "Space" && !spaceHeld) {
      spaceHeld = true;
      e.preventDefault();
      return;
    }
    const tool = TOOLS[activeToolId];
    if (tool?.key?.(editor, e)) {
      e.preventDefault();
      drawOverlay();
    }
    if (e.key === "Escape") {
      tool?.cancel?.(editor);
      drawOverlay();
    }
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.code === "Space") spaceHeld = false;
  }

  onMount(() => {
    const ro = new ResizeObserver(() => {
      dpr = window.devicePixelRatio || 1;
      const r = host.getBoundingClientRect();
      editor.viewport = { width: Math.max(1, r.width), height: Math.max(1, r.height) };
      for (const c of [base, overlay]) {
        c.width = Math.round(r.width * dpr);
        c.height = Math.round(r.height * dpr);
      }
      drawFrame();
      renderNow();
    });
    ro.observe(host);
    let raf = 0;
    let lastAnts = 0;
    const tick = (t: number) => {
      if (edges && t - lastAnts > 120) {
        antsPhase = (antsPhase + 2) % 16;
        lastAnts = t;
        drawOverlay();
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  });

  // Re-render whenever the document, the view or an explicit tick changes.
  // Changes arriving in quick succession (a drag, a slider, a pinch) render
  // at half resolution, then one full-resolution frame follows when they stop.
  let lastChange = 0;
  let refineTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void editor.summary?.revision;
    void editor.view.cx;
    void editor.view.cy;
    void editor.view.zoom;
    void editor.renderTick;
    void editor.hasDocument;
    const now = performance.now();
    const rapid = now - lastChange < 180 && dpr * editor.viewport.width * editor.viewport.height > 900_000;
    lastChange = now;
    drawFrame();
    clearTimeout(refineTimer);
    if (rapid) {
      renderNow(0.5);
      refineTimer = setTimeout(() => renderNow(1), 220);
    } else {
      renderNow(1);
    }
  });

  $effect(() => {
    void editor.overlayTick;
    void activeToolId;
    drawOverlay();
  });

  // Switching tools mid-gesture cancels the old one.
  let prevTool = "";
  $effect(() => {
    const id = activeToolId;
    if (prevTool && prevTool !== id) {
      TOOLS[prevTool]?.deactivate?.(editor);
      TOOLS[id]?.activate?.(editor);
    }
    prevTool = id;
  });
</script>

<div class="ops-canvas" bind:this={host} data-testid="canvas">
  <canvas class="ops-canvas__base" bind:this={base}></canvas>
  <canvas
    class="ops-canvas__overlay"
    bind:this={overlay}
    style:cursor={cursor}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onwheel={onWheel}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
</div>

<style>
  .ops-canvas {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--ops-pasteboard);
    touch-action: none;
    --ops-checker-light: #ffffff;
    --ops-checker-dark: #e3e3e0;
  }
  .ops-canvas canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
