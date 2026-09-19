<script lang="ts">
  // The document viewport: renders the composite through the engine, draws
  // selection outlines and tool overlays, and turns pointer input into tool
  // calls in document coordinates. Canvas-wide gestures live here: the
  // right-drag brush size/hardness scrub, edge autoscroll while dragging,
  // and Cmd/Ctrl+arrow pixel nudges.
  import { onMount } from "svelte";
  import { editor } from "../lib/editor.svelte";
  import { TOOLS } from "../tools/registry";
  import type { Tool, ToolPointer } from "../tools/types";
  import { drawLabel, mods } from "../tools/common";
  import { setSizeFor as setBrushSizeFor, sizeFor as brushSizeFor, toolSettings } from "../tools/settings.svelte";
  import { liquifySettings, setLiquifySize } from "../tools/liquify.svelte";

  // Liquify keeps its own brush size; every other brush tool shares the
  // settings store. Right-drag resizing goes through these two.
  const sizeFor = (tool: string) => (tool === "liquify" ? liquifySettings.size : brushSizeFor(tool));
  const setSizeFor = (tool: string, v: number) => (tool === "liquify" ? setLiquifySize(v) : setBrushSizeFor(tool, v));
  import { nudgeSelectedPixels } from "../tools/move";

  /** Tools that paint with a round tip: right-drag scrubs its size and hardness. */
  const BRUSH_TOOLS = new Set(["brush", "pencil", "eraser", "clone", "heal", "spot-heal", "remove", "dodge", "burn", "sponge", "blur-brush", "sharpen-brush", "smudge", "liquify", "quick-select"]);
  /** Tools whose drags pan the view when the pointer goes past the edge. */
  const AUTOSCROLL_TOOLS = new Set(["marquee-rect", "marquee-ellipse", "lasso", "move", "crop", "transform", "object-select"]);
  /** Tool hooks the contract does not name (Alt released ends an open lasso). */
  type ToolExt = Tool & { keyup?(ed: typeof editor, e: KeyboardEvent): boolean };

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
  let altHeld = $state(false);
  let pressed = false;
  /** Last pointer event of the current drag (autoscroll and key-only re-dispatch). */
  let lastDragEvent: PointerEvent | null = null;
  let autoscrollRaf = 0;
  let pointerDownTool: string | null = null;

  // Selection outline: edge pixels in device space for the current frame.
  let edges: Int32Array | null = null;
  let edgeFrame: { x: number; y: number; scale: number; w: number; h: number; i0: number; j0: number } | null = null;
  let antsPhase = 0;
  let antsImg: ImageData | null = null;
  let antsWritten: number[] = [];

  const activeToolId = $derived(spaceHeld ? "hand" : (toolOverride ?? editor.tool));
  const cursor = $derived(activeToolId === "zoom" && altHeld && !editor.cursor ? "zoom-out" : (editor.cursor ?? TOOLS[activeToolId]?.cursor ?? "default"));

  // ---------------------------------------------------------------------
  // Right-drag brush scrub (Photoshop: Ctrl+Opt-drag on macOS, Alt+right-drag
  // on Windows, both with vertical hardness; Compositor: right-drag, with
  // Shift for hardness). Changes the tool options only: no history step.

  interface Scrub {
    tool: string;
    /** Where the tip stays pinned (viewport px). */
    px: number;
    py: number;
    /** Baseline of the current axis (rebased when Shift toggles). */
    vx: number;
    vy: number;
    size0: number;
    hard0: number;
    /** Photoshop's HUD: horizontal size and vertical hardness together. */
    hud: boolean;
    shift: boolean;
  }
  let scrub: Scrub | null = null;

  function hardnessKey(tool: string): "brushHardness" | "eraserHardness" | null {
    if (tool === "pencil") return null;
    return tool === "eraser" ? "eraserHardness" : "brushHardness";
  }

  function hardnessOf(tool: string): number {
    const k = hardnessKey(tool);
    return k ? toolSettings[k] : 1;
  }

  function startScrub(e: PointerEvent, tool: string) {
    const p = pointer(e);
    scrub = { tool, px: p.vx, py: p.vy, vx: p.vx, vy: p.vy, size0: sizeFor(tool), hard0: hardnessOf(tool), hud: e.button === 0 || e.altKey, shift: e.shiftKey };
    drawOverlay();
  }

  function moveScrub(e: PointerEvent) {
    const sc = scrub!;
    const p = pointer(e);
    if (!sc.hud && e.shiftKey !== sc.shift) {
      // Switching axis keeps what the other one set.
      Object.assign(sc, { vx: p.vx, vy: p.vy, size0: sizeFor(sc.tool), hard0: hardnessOf(sc.tool), shift: e.shiftKey });
    }
    const dx = p.vx - sc.vx;
    const dy = p.vy - sc.vy;
    const k = hardnessKey(sc.tool);
    // The circle's edge follows the pointer: radius grows by dx view px.
    if (sc.hud || !sc.shift) setSizeFor(sc.tool, sc.size0 + (2 * dx) / editor.view.zoom);
    if (k && (sc.hud || sc.shift)) {
      const h = sc.hud ? sc.hard0 - dy / 200 : sc.hard0 + dx / 200;
      toolSettings[k] = Math.round(Math.min(1, Math.max(0, h)) * 100) / 100;
    }
    drawOverlay();
  }

  function drawScrub(ctx: CanvasRenderingContext2D) {
    const sc = scrub;
    if (!sc) return;
    const size = sizeFor(sc.tool);
    const hard = hardnessOf(sc.tool);
    const r = Math.max(1, (size / 2) * editor.view.zoom);
    ctx.save();
    ctx.lineWidth = 1;
    for (const [col, off] of [["rgba(0,0,0,0.85)", 0], ["rgba(255,255,255,0.9)", 1]] as const) {
      ctx.strokeStyle = col;
      ctx.beginPath();
      ctx.arc(sc.px, sc.py, r + off, 0, Math.PI * 2);
      ctx.stroke();
    }
    // The hard core: where the tip is fully opaque.
    if (hard < 1 && r * hard > 1) {
      ctx.setLineDash([3, 3]);
      for (const [col, off] of [["rgba(0,0,0,0.8)", 0], ["rgba(255,255,255,0.9)", 3]] as const) {
        ctx.strokeStyle = col;
        ctx.lineDashOffset = off;
        ctx.beginPath();
        ctx.arc(sc.px, sc.py, r * hard, 0, Math.PI * 2);
        ctx.stroke();
      }
    }
    ctx.restore();
    const hk = hardnessKey(sc.tool);
    drawLabel(ctx, hk ? `Ø ${size} px · ${Math.round(hard * 100)}% hard` : `Ø ${size} px`, sc.px + Math.min(r, 400), sc.py - 24);
  }

  // ---------------------------------------------------------------------
  // Edge autoscroll: a drag held past the viewport edge pans the view at a
  // speed that grows with the overshoot, and the drag keeps extending.

  function autoscrollTick() {
    autoscrollRaf = 0;
    const e = lastDragEvent;
    const id = pointerDownTool;
    if (!pressed || !e || !id || !AUTOSCROLL_TOOLS.has(id) || !overlay || !editor.summary) return;
    const r = overlay.getBoundingClientRect();
    const M = 4;
    const speed = (past: number) => (past > 0 ? Math.min(40, 2 + past * 0.4) : 0);
    let px = speed(e.clientX - (r.right - M)) - speed(r.left + M - e.clientX);
    let py = speed(e.clientY - (r.bottom - M)) - speed(r.top + M - e.clientY);
    // Stop once the document's far edge is well inside the view.
    const s = editor.summary;
    const tl = editor.toView(0, 0);
    const br = editor.toView(s.width, s.height);
    const slack = 64;
    if (px > 0 && br.x < r.width - slack) px = 0;
    if (px < 0 && tl.x > slack) px = 0;
    if (py > 0 && br.y < r.height - slack) py = 0;
    if (py < 0 && tl.y > slack) py = 0;
    if (!px && !py) return;
    editor.panBy(-px, -py);
    TOOLS[id]?.move?.(editor, pointer(e), true);
    drawOverlay();
    autoscrollRaf = requestAnimationFrame(autoscrollTick);
  }

  function maybeAutoscroll() {
    if (!autoscrollRaf) autoscrollRaf = requestAnimationFrame(autoscrollTick);
  }

  function stopAutoscroll() {
    if (autoscrollRaf) cancelAnimationFrame(autoscrollRaf);
    autoscrollRaf = 0;
    lastDragEvent = null;
  }

  function trackMods(e: KeyboardEvent | PointerEvent | WheelEvent) {
    mods.ctrl = e.ctrlKey;
    mods.alt = e.altKey;
    mods.shift = e.shiftKey;
    mods.meta = e.metaKey;
  }

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
    // Clip to whole device pixels of the document so a partly covered edge
    // column never shows the checkerboard through as a light line.
    const clipL = Math.max(0, Math.ceil(left - 1e-6));
    const clipT = Math.max(0, Math.ceil(top - 1e-6));
    const clipR = Math.min(base.width, Math.floor(left + s.width * g.scale + 1e-6));
    const clipB = Math.min(base.height, Math.floor(top + s.height * g.scale + 1e-6));
    ctx.save();
    ctx.beginPath();
    ctx.rect(clipL, clipT, Math.max(0, clipR - clipL), Math.max(0, clipB - clipT));
    ctx.clip();
    ctx.save();
    ctx.fillStyle = checker(ctx);
    ctx.translate(Math.round(left), Math.round(top));
    ctx.fillRect(-1, -1, Math.round(s.width * g.scale) + 2, Math.round(s.height * g.scale) + 2);
    ctx.restore();
    if (frame) {
      const k = g.scale / frame.scale;
      const dx = (frame.x - g.x0) * g.scale;
      const dy = (frame.y - g.y0) * g.scale;
      ctx.imageSmoothingEnabled = k < 1;
      ctx.drawImage(frame.bitmap, dx, dy, frame.bitmap.width * k, frame.bitmap.height * k);
    }
    ctx.restore();
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
        const bitmap = await createImageBitmap(new ImageData(data as Uint8ClampedArray<ArrayBuffer>, g.w, g.h), { premultiplyAlpha: "premultiply" });
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
    if (tool?.overlay || scrub) {
      ctx.save();
      ctx.scale(dpr, dpr);
      tool?.overlay?.(editor, ctx);
      drawScrub(ctx);
      ctx.restore();
    }
  }

  function pointer(e: PointerEvent | WheelEvent): ToolPointer {
    const r = overlay.getBoundingClientRect();
    const vx = e.clientX - r.left;
    const vy = e.clientY - r.top;
    const d = editor.toDoc(vx, vy);
    const isMac = navigator.platform.toLowerCase().includes("mac");
    trackMods(e);
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
    trackMods(e);
    const hud = e.pointerType !== "touch" && (e.button === 2 || (e.button === 0 && e.ctrlKey && e.altKey));
    if (hud && BRUSH_TOOLS.has(activeToolId) && !pressed) {
      startScrub(e, activeToolId);
      return;
    }
    // The right button (and extra mouse buttons) never reach a tool: a
    // right-drag must not paint, select or move.
    if (e.button !== 0 && e.button !== 1) {
      overlay.releasePointerCapture?.(e.pointerId);
      return;
    }
    if (e.button === 1) {
      pointerDownTool = "hand";
    } else {
      pointerDownTool = activeToolId;
    }
    pressed = true;
    lastDragEvent = e;
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
    if (scrub) {
      moveScrub(e);
      return;
    }
    const events = pressed && "getCoalescedEvents" in e ? e.getCoalescedEvents() : [e];
    const tool = TOOLS[pressed ? pointerDownTool ?? activeToolId : activeToolId];
    for (const ev of events.length ? events : [e]) tool?.move?.(editor, pointer(ev), pressed);
    if (pressed) {
      lastDragEvent = e;
      maybeAutoscroll();
    }
    drawOverlay();
  }

  function onPointerUp(e: PointerEvent) {
    touches.delete(e.pointerId);
    if (pinch) {
      if (touches.size < 2) pinch = null;
      return;
    }
    if (scrub) {
      scrub = null;
      drawOverlay();
      return;
    }
    stopAutoscroll();
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

  const ARROWS: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };

  /** A modifier changed mid-drag without the pointer moving: re-run the drag. */
  function modifierChanged(e: KeyboardEvent) {
    trackMods(e);
    if (e.key === "Alt") altHeld = e.type === "keydown";
    if (!["Shift", "Alt", "Control", "Meta"].includes(e.key)) return;
    if (pressed && lastDragEvent && pointerDownTool) {
      const held = { shift: e.shiftKey, alt: e.altKey };
      const p = { ...pointer(lastDragEvent), ...held };
      trackMods(e);
      TOOLS[pointerDownTool]?.move?.(editor, p, true);
      drawOverlay();
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    modifierChanged(e);
    if (isTyping(e.target) || e.defaultPrevented) return;
    if (e.code === "Space" && !spaceHeld) {
      spaceHeld = true;
      e.preventDefault();
      return;
    }
    // Cmd/Ctrl+arrow nudges the selected pixels from any tool (Shift ×10).
    const mac = navigator.platform.toLowerCase().includes("mac");
    const arrow = ARROWS[e.key];
    if (arrow && (mac ? e.metaKey : e.ctrlKey) && !e.altKey && editor.summary?.selection && !["transform", "crop", "perspective-crop"].includes(activeToolId)) {
      e.preventDefault();
      const k = e.shiftKey ? 10 : 1;
      void nudgeSelectedPixels(editor, arrow[0] * k, arrow[1] * k);
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
    modifierChanged(e);
    if (e.code === "Space") spaceHeld = false;
    if (!isTyping(e.target) && (TOOLS[activeToolId] as ToolExt | undefined)?.keyup?.(editor, e)) drawOverlay();
  }

  function onBlur() {
    Object.assign(mods, { ctrl: false, alt: false, shift: false, meta: false });
    altHeld = false;
    scrub = null;
  }

  onMount(() => {
    editor.canvasHost = host;
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
    window.addEventListener("blur", onBlur);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
      stopAutoscroll();
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
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
      editor.cursor = null;
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
