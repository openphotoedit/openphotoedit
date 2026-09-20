<script lang="ts">
  // The sidebar: marks for the focused photo, its culling signals, and what
  // the camera recorded.
  import Flag from "@lucide/svelte/icons/flag";
  import FlagOff from "@lucide/svelte/icons/flag-off";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import { t } from "../../lib/i18n";
  import { tooltip } from "../../ui/tooltip";
  import { formatAperture, formatExposure, formatFocal, orientedSize } from "../exif";
  import { library } from "../store.svelte";
  import { LABEL_NAMES } from "../xmp";
  import Badges from "./Badges.svelte";
  import Marks from "./Marks.svelte";

  const item = $derived(library.focused);
  const m = $derived(item ? library.marksOf(item.path) : null);
  const exif = $derived(item ? library.meta.get(item.path) : undefined);
  const cull = $derived(item ? library.cull.get(item.path) : undefined);
  const count = $derived(library.targets().length);

  function bytes(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function when(s?: string) {
    if (!s) return undefined;
    const d = new Date(s.length === 19 ? s + "Z" : s);
    if (Number.isNaN(+d)) return s;
    return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short", timeZone: s.length === 19 ? "UTC" : undefined });
  }

  const rows = $derived.by(() => {
    if (!item) return [];
    const size = exif ? orientedSize(exif) : null;
    const focal = formatFocal(exif?.focalLength);
    const r: [string, string | undefined][] = [
      [t("Camera"), [exif?.make && !exif.model?.toLowerCase().startsWith(exif.make.toLowerCase().split(" ")[0]) ? exif.make : "", exif?.model].filter(Boolean).join(" ") || undefined],
      [t("Lens"), exif?.lens],
      [t("Exposure"), [formatExposure(exif?.exposureTime), formatAperture(exif?.fNumber), exif?.iso ? `ISO ${exif.iso}` : ""].filter(Boolean).join("  ·  ") || undefined],
      [t("Focal length"), focal ? (exif?.focalLength35 && Math.round(exif.focalLength35) !== Math.round(exif.focalLength ?? 0) ? `${focal} (${exif.focalLength35} mm eq.)` : focal) : undefined],
      [t("Taken"), when(exif?.dateTaken)],
      [t("Dimensions"), size ? `${size.width} × ${size.height}` : undefined],
      [t("File size"), bytes(item.size)],
      [t("Type"), item.kind === "raw" ? `${item.ext.toUpperCase()} (${t("raw")})` : item.ext.toUpperCase()],
      [t("Modified"), new Date(item.lastModified).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })],
      [t("Sidecar"), item.hasSidecar ? `${item.name.replace(/\.[^.]+$/, "")}.xmp` : undefined],
    ];
    return r.filter((x) => x[1]);
  });
</script>

<aside class="side lib-scroll" data-testid="lib-meta">
  {#if !item || !m}
    <p class="oa-empty pad">{t("Select a photo to see its details.")}</p>
  {:else}
    <section>
      <div class="title" title={item.path}>{item.name}</div>
      {#if item.path.includes("/")}<div class="path">{item.path.slice(0, item.path.lastIndexOf("/"))}</div>{/if}
      {#if count > 1}<div class="multi">{t("Marks apply to {n} selected photos", { n: count })}</div>{/if}
    </section>

    <section class="marks">
      <div class="row">
        <Marks marks={m} editable onrate={(r) => library.setRating(r, false)} size={14} />
        <span class="grow"></span>
        <span class="lib-kbd" use:tooltip={t("Press 0–5 to rate")}>0–5</span>
      </div>
      <div class="row">
        <div class="seg" role="group" aria-label={t("Flag")}>
          <button type="button" class:on={m.flag === 1} onclick={() => library.setFlag(1, false)} use:tooltip={{ text: t("Pick"), shortcut: "P" }} data-testid="lib-flag-pick"><Flag size={13} fill={m.flag === 1 ? "currentColor" : "none"} /></button>
          <button type="button" class:on={m.flag === 0} onclick={() => library.setFlag(0, false)} use:tooltip={{ text: t("Unflag"), shortcut: "U" }}><CircleSlash size={13} /></button>
          <button type="button" class:on={m.flag === -1} class="reject" onclick={() => library.setFlag(-1, false)} use:tooltip={{ text: t("Reject"), shortcut: "X" }} data-testid="lib-flag-reject"><FlagOff size={13} /></button>
        </div>
        <span class="grow"></span>
        <div class="labels" role="group" aria-label={t("Colour label")}>
          {#each [1, 2, 3, 4, 5] as l (l)}
            <button type="button" class="label" class:on={m.label === l} onclick={() => library.toggleLabel(l, false)} use:tooltip={{ text: t(LABEL_NAMES[l]), shortcut: l <= 4 ? String(l + 5) : undefined }} aria-label={t(LABEL_NAMES[l])} aria-pressed={m.label === l}>
              <span class="lib-dot" data-label={l}></span>
            </button>
          {/each}
        </div>
      </div>
    </section>

    <section>
      <h3 class="ops-section-title">{t("Culling")}</h3>
      {#if cull}
        <div class="badges"><Badges result={cull} /></div>
        <dl>
          <dt>{t("Focus")}</dt>
          <dd data-testid="lib-sharpness">{cull.sharpness.toFixed(1)}{#if cull.sharpnessRel != null}<span class="dim"> · {Math.round(cull.sharpnessRel * 100)}% {t("of typical")}</span>{/if}</dd>
          <dt>{t("Brightness")}</dt>
          <dd>{Math.round(cull.mean * 100)}%<span class="dim"> · {t("clipped")} {(cull.clipLow * 100).toFixed(1)}% / {(cull.clipHigh * 100).toFixed(1)}%</span></dd>
          <dt>{t("Faces")}</dt>
          <dd>{cull.faces == null ? t("Not checked") : cull.eyesClosed ? t("{n} ({c} with eyes closed)", { n: cull.faces, c: cull.eyesClosed }) : cull.faces}</dd>
          {#if cull.groupSize}
            <dt>{t("Similar")}</dt>
            <dd>{t("{n} shots", { n: cull.groupSize })}{#if cull.badges.includes("best")}<span class="dim"> · {t("best of group")}</span>{/if}</dd>
          {/if}
        </dl>
      {:else}
        <p class="oa-empty">{t("Run assisted culling to check focus, exposure, faces and similar shots.")}</p>
      {/if}
    </section>

    <section>
      <h3 class="ops-section-title">{t("Metadata")}</h3>
      <dl>
        {#each rows as [k, v] (k)}
          <dt>{k}</dt>
          <dd>{v}</dd>
        {/each}
      </dl>
    </section>
  {/if}
</aside>

<style>
  .side {
    width: var(--lib-side-w);
    flex: 0 0 auto;
    overflow-y: auto;
    border-left: var(--border-width) solid var(--border-hairline);
    background: var(--bg-subtle);
  }
  section {
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-width) solid var(--border-hairline);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .pad {
    padding: var(--space-4);
  }
  .title {
    font: var(--weight-medium) var(--text-sm) / 1.3 var(--font-sans);
    color: var(--text-strong);
    overflow-wrap: anywhere;
  }
  .path,
  .multi {
    font: var(--type-caption);
    color: var(--text-faint);
    margin-top: -4px;
    overflow-wrap: anywhere;
  }
  .multi {
    color: var(--text-muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    background: var(--bg-sunken);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-xs);
  }
  .seg button,
  .label {
    display: grid;
    place-items: center;
    width: 26px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .seg button:hover,
  .label:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }
  .seg button.on {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .seg button.reject.on {
    color: var(--danger-fg);
  }
  .labels {
    display: inline-flex;
    gap: 0;
  }
  .label {
    width: 20px;
  }
  .label.on {
    background: var(--surface-active);
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px var(--space-3);
    margin: 0;
    font: var(--type-caption);
  }
  dt {
    color: var(--text-faint);
    white-space: nowrap;
  }
  dd {
    margin: 0;
    color: var(--text-body);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .dim {
    color: var(--text-faint);
  }
  .badges {
    min-height: 0;
  }
</style>
