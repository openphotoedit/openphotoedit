<script lang="ts">
  // Help › Keyboard Shortcuts, Help › About, and Layer › New Fill Layer.
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import BrandMark from "../../ui/BrandMark.svelte";
  import ColorPicker from "../../ui/ColorPicker.svelte";
  import Dialog from "../../ui/Dialog.svelte";
  import GradientEditor from "../../ui/GradientEditor.svelte";
  import Select from "../../ui/Select.svelte";
  import Slider from "../../ui/Slider.svelte";
  import { formatShortcut } from "../../ui/platform";
  import type { GradientStop, Rgba8 } from "../../engine/types";
  import { ACTIONS, MENUS } from "../actions.svelte";
  import { run } from "../engine.svelte";
  import { pro, type DialogSpec } from "../state.svelte";
  import { toolBridge, toolMeta } from "../tools.svelte";
  import { isItem, subItems, type MenuEntry } from "../../ui/menu";

  let { spec }: { spec: DialogSpec } = $props();
  const close = () => pro.close();

  // ---- Shortcuts: every menu item that has one, grouped by menu, plus tools.
  let query = $state("");
  function collect(entries: MenuEntry[], out: { label: string; shortcut: string }[], prefix = "") {
    for (const e of entries) {
      if (!isItem(e)) continue;
      if (e.submenu) collect(subItems(e) ?? [], out, `${prefix}${e.label} › `);
      else if (e.shortcut) out.push({ label: `${prefix}${e.label}`, shortcut: e.shortcut });
    }
  }
  const sections = (() => spec.kind === "shortcuts"
    ? [
        ...MENUS.map((m) => {
          const rows: { label: string; shortcut: string }[] = [];
          collect(m.items(), rows);
          return { title: m.label, rows };
        }),
        {
          title: t("Tools"),
          rows: toolBridge.groups.flatMap((g) => g.tools.map((id) => toolMeta(id))).filter((m) => m.shortcut).map((m) => ({ label: m.label, shortcut: m.shortcut! })),
        },
        {
          title: t("Canvas"),
          rows: [
            { label: t("Pan (hold)"), shortcut: "Space" },
            { label: t("Switch foreground and background colours"), shortcut: "X" },
            { label: t("Default colours"), shortcut: "D" },
            { label: t("Quick Mask mode"), shortcut: "Q" },
            { label: t("Show or hide panels"), shortcut: "Tab" },
          ],
        },
      ]
    : [])();
  const filtered = $derived(
    sections.map((s) => ({ ...s, rows: s.rows.filter((r) => !query || r.label.toLowerCase().includes(query.toLowerCase())) })).filter((s) => s.rows.length),
  );
  void ACTIONS;

  // ---- Fill layer
  let solid = $state<Rgba8>({ ...editor.primary });
  let stops = $state<GradientStop[]>([
    { pos: 0, color: { ...editor.primary } },
    { pos: 1, color: { ...editor.secondary } },
  ]);
  let gradient = $state<"linear" | "radial" | "angle" | "reflected" | "diamond">("linear");
  let angle = $state(90);

  async function addFill() {
    close();
    const s = editor.summary!;
    let fill: Record<string, unknown>;
    if (spec.kind === "fill-layer" && spec.fill === "gradient") {
      const rad = (angle * Math.PI) / 180;
      const cx = s.width / 2;
      const cy = s.height / 2;
      const half = (Math.abs(Math.cos(rad)) * s.width + Math.abs(Math.sin(rad)) * s.height) / 2;
      fill = {
        kind: "gradient",
        stops: $state.snapshot(stops),
        gradient,
        from: { x: cx - Math.cos(rad) * half, y: cy + Math.sin(rad) * half },
        to: { x: cx + Math.cos(rad) * half, y: cy - Math.sin(rad) * half },
      };
    } else fill = { kind: "solid", color: $state.snapshot(solid) };
    await run({ op: "layer.add-fill", fill, name: spec.kind === "fill-layer" && spec.fill === "gradient" ? t("Gradient Fill") : t("Color Fill"), above: s.active ?? undefined }, undefined, t("Fill layer"));
  }
</script>

{#if spec.kind === "shortcuts"}
  <Dialog title={t("Keyboard Shortcuts")} width={640} testid="shortcuts-dialog" onclose={close}>
    <input class="ops-field search" placeholder={t("Search commands")} aria-label={t("Search commands")} bind:value={query} data-autofocus />
    <div class="sheet">
      {#each filtered as s (s.title)}
        <section>
          <h3 class="ops-section-title">{s.title}</h3>
          {#each s.rows as r (r.label)}
            <div class="sc"><span>{r.label}</span><kbd>{formatShortcut(r.shortcut)}</kbd></div>
          {/each}
        </section>
      {/each}
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--primary" onclick={close}>{t("Done")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "about"}
  <Dialog title={t("About")} width={380} testid="about-dialog" onclose={close}>
    <div class="about">
      <BrandMark variant="lockup" size={40} />
      <p>{t("A layered photo editor that runs entirely in your browser.")}</p>
      <p class="ops-note">{t("Everything happens on this device. Your images are never uploaded, and there is no account.")}</p>
      <p class="ops-note">{t("Free software under the GNU Affero General Public License, version 3 or later (AGPL-3.0-or-later).")}</p>
      <p class="ops-note">{t("Photoshop is a trademark of Adobe. This project is not affiliated with Adobe.")}</p>
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--primary" onclick={close}>{t("Close")}</button>
    {/snippet}
  </Dialog>
{:else if spec.kind === "fill-layer"}
  <Dialog title={spec.fill === "gradient" ? t("Gradient Fill") : t("Solid Color Fill")} width={320} onclose={close} onsubmit={addFill}>
    <div class="ops-stack">
      {#if spec.fill === "gradient"}
        <GradientEditor {stops} onchange={(s) => (stops = s)} />
        <div class="ops-row">
          <span class="ops-label">{t("Style")}</span>
          <Select
            ariaLabel={t("Style")}
            value={gradient}
            width={120}
            options={["linear", "radial", "angle", "reflected", "diamond"].map((g) => ({ value: g as typeof gradient, label: t(g.charAt(0).toUpperCase() + g.slice(1)) }))}
            onchange={(v) => (gradient = v)}
          />
        </div>
        <Slider label={t("Angle")} value={angle} min={-180} max={180} unit="°" defaultValue={90} oninput={(v) => (angle = v)} />
      {:else}
        <ColorPicker value={solid} height={160} oninput={(c) => (solid = c)} />
      {/if}
    </div>
    {#snippet footer()}
      <button type="button" class="oa-btn oa-btn--secondary" onclick={close}>{t("Cancel")}</button>
      <button type="button" class="oa-btn oa-btn--primary" data-testid="dialog-ok" onclick={addFill}>{t("OK")}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .search {
    width: 100%;
    margin-bottom: 12px;
  }
  .sheet {
    columns: 2;
    column-gap: 24px;
  }
  section {
    break-inside: avoid;
    margin-bottom: 14px;
  }
  h3 {
    margin: 0 0 4px;
  }
  .sc {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 3px 0;
    border-bottom: 1px solid var(--border-hairline);
    font: var(--type-caption);
    color: var(--text-body);
  }
  kbd {
    font: var(--weight-regular) var(--text-2xs) / 1.4 var(--font-sans);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .about {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }
  .about p {
    margin: 0;
    font: var(--weight-regular) var(--text-sm) / 1.5 var(--font-sans);
    color: var(--text-body);
  }
  .about .ops-note {
    font: var(--type-caption);
    color: var(--text-muted);
  }
</style>
