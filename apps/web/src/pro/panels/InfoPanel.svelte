<script lang="ts">
  // Info: colour under the cursor (RGB, HSB, hex), cursor position,
  // selection size and document size.
  import Crosshair from "@lucide/svelte/icons/crosshair";
  import SquareDashed from "@lucide/svelte/icons/square-dashed";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { css, rgbToHsb, toHex } from "../../ui/color";
  import { pro } from "../state.svelte";

  const c = $derived(pro.cursorColor);
  const hsb = $derived(c ? rgbToHsb(c) : null);
  const sel = $derived(editor.summary?.selection?.bounds ?? null);
</script>

<div class="panel" data-testid="info-panel">
  <div class="cols">
    <div class="block">
      <div class="title"><span class="chip" style:background={c ? css(c) : "transparent"}></span>RGB</div>
      <dl>
        <dt>R</dt><dd>{c ? c.r : "—"}</dd>
        <dt>G</dt><dd>{c ? c.g : "—"}</dd>
        <dt>B</dt><dd>{c ? c.b : "—"}</dd>
        <dt>A</dt><dd>{c ? Math.round(((c.a ?? 255) / 255) * 100) + "%" : "—"}</dd>
      </dl>
    </div>
    <div class="block">
      <div class="title">HSB</div>
      <dl>
        <dt>H</dt><dd>{hsb ? Math.round(hsb.h) + "°" : "—"}</dd>
        <dt>S</dt><dd>{hsb ? Math.round(hsb.s) + "%" : "—"}</dd>
        <dt>B</dt><dd>{hsb ? Math.round(hsb.b) + "%" : "—"}</dd>
        <dt>#</dt><dd class="mono">{c ? toHex(c) : "—"}</dd>
      </dl>
    </div>
  </div>
  <hr class="ops-hr" />
  <div class="cols">
    <div class="block">
      <div class="title"><Crosshair size={12} />{t("Position")}</div>
      <dl>
        <dt>X</dt><dd>{pro.cursor ? Math.floor(pro.cursor.x) : "—"}</dd>
        <dt>Y</dt><dd>{pro.cursor ? Math.floor(pro.cursor.y) : "—"}</dd>
      </dl>
    </div>
    <div class="block">
      <div class="title"><SquareDashed size={12} />{t("Selection")}</div>
      <dl>
        <dt>W</dt><dd>{sel ? sel.w : "—"}</dd>
        <dt>H</dt><dd>{sel ? sel.h : "—"}</dd>
      </dl>
    </div>
  </div>
  {#if editor.summary && editor.hasDocument}
    <hr class="ops-hr" />
    <p class="doc">{t("Document")}: {editor.summary.width} × {editor.summary.height} px, {editor.summary.resolution} ppi, {t("{n} layers", { n: editor.summary.layers.length })}</p>
  {/if}
</div>

<style>
  .panel {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font: var(--type-caption);
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
    color: var(--text-muted);
  }
  .chip {
    width: 10px;
    height: 10px;
    border: 1px solid var(--border-strong);
    border-radius: 2px;
  }
  dl {
    display: grid;
    grid-template-columns: 14px 1fr;
    gap: 2px 6px;
    margin: 0;
  }
  dt {
    color: var(--text-faint);
  }
  dd {
    margin: 0;
    color: var(--text-strong);
    font-variant-numeric: tabular-nums;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .doc {
    margin: 0;
    color: var(--text-muted);
  }
</style>
