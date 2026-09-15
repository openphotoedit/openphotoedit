<script lang="ts">
  // The in-place text editor: a textarea laid exactly over the text's place
  // on the canvas, styled from the text options so typing looks like the
  // result. Mounted into document.body by the text tool.
  import { onMount } from "svelte";
  import type { EditorStore } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { cssFont, layoutText } from "../lib/text";
  import { canvasHost, css } from "./common";
  import { cancelText, commitText, sessionData, textEdit } from "./text.svelte";

  let { ed }: { ed: EditorStore } = $props();

  let area: HTMLTextAreaElement | undefined = $state();
  let hostRect = $state({ left: 0, top: 0 });

  function measureHost() {
    const r = canvasHost()?.getBoundingClientRect();
    if (r) hostRect = { left: r.left, top: r.top };
  }

  onMount(() => {
    measureHost();
    const onResize = () => measureHost();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    requestAnimationFrame(() => area?.focus({ preventScroll: true }));
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  });

  const geom = $derived.by(() => {
    const s = textEdit.session;
    if (!s) return null;
    void ed.viewport.width;
    void ed.viewport.height;
    const data = sessionData({ ...s, text: s.text || " " });
    const z = ed.view.zoom;
    const lay = layoutText(data);
    const pad = data.background ? data.padding : 0;
    const v = ed.toView(s.x - pad, s.y - pad);
    const caret = Math.max(4, data.font_size * 0.6);
    const w = (s.boxWidth != null ? s.boxWidth : lay.width + caret) + pad * 2;
    const h = lay.height + pad * 2;
    return {
      left: hostRect.left + v.x,
      top: hostRect.top + v.y,
      width: w * z,
      height: h * z,
      pad: pad * z,
      font: cssFont(data, z),
      lineHeight: data.line_height,
      letterSpacing: data.letter_spacing * z,
      color: css(data.color),
      background: data.background ? css(data.background) : "transparent",
      radius: data.background ? Math.min((h * z) / 2, Math.max(4 * z, pad * z * 0.75)) : 0,
      stroke: data.stroke ? `${data.stroke_width * z * 2}px ${css(data.stroke)}` : "",
      align: data.align,
      rotation: s.rotation,
      wrap: s.boxWidth != null,
    };
  });

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      void cancelText(ed);
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void commitText(ed);
    }
  }

  // Focus again whenever a new session opens.
  let focusedFor: object | null = null;
  $effect(() => {
    const s = textEdit.session;
    if (s && area && focusedFor !== s) {
      focusedFor = s;
      const el = area;
      requestAnimationFrame(() => el.focus({ preventScroll: true }));
    }
  });
</script>

{#if textEdit.session && geom}
  <textarea
    bind:this={area}
    bind:value={textEdit.session.text}
    class="ops-text-editor"
    class:wrap={geom.wrap}
    data-testid="text-editor"
    aria-label={t("Text")}
    spellcheck="false"
    autocomplete="off"
    wrap={geom.wrap ? "soft" : "off"}
    placeholder={t("Type here")}
    onkeydown={onKey}
    onpointerdown={(e) => e.stopPropagation()}
    style:left="{geom.left}px"
    style:top="{geom.top}px"
    style:width="{geom.width}px"
    style:height="{geom.height}px"
    style:padding="{geom.pad}px"
    style:font={geom.font}
    style:line-height={geom.lineHeight}
    style:letter-spacing="{geom.letterSpacing}px"
    style:color={geom.color}
    style:background={geom.background}
    style:border-radius="{geom.radius}px"
    style:text-align={geom.align}
    style:transform="rotate({geom.rotation}deg)"
    style:-webkit-text-stroke={geom.stroke}
  ></textarea>
{/if}

<style>
  .ops-text-editor {
    position: fixed;
    z-index: 40;
    box-sizing: border-box;
    margin: 0;
    border: 0;
    outline: 1px dashed rgba(20, 115, 230, 0.9);
    outline-offset: 2px;
    resize: none;
    overflow: hidden;
    white-space: pre;
    caret-color: #1473e6;
    transform-origin: center;
    paint-order: stroke fill;
  }
  .ops-text-editor.wrap {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .ops-text-editor::placeholder {
    color: rgba(128, 128, 128, 0.7);
  }
</style>
