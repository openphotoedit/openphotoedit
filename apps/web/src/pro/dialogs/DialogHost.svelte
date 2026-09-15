<script lang="ts">
  // Renders whichever dialog `pro.dialog` names. Keyed so reopening starts fresh.
  import { editor } from "../../lib/editor.svelte";
  import { pro } from "../state.svelte";
  import AdjustmentDialog from "./AdjustmentDialog.svelte";
  import CanvasSizeDialog from "./CanvasSizeDialog.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import FilterDialog from "./FilterDialog.svelte";
  import ImageSizeDialog from "./ImageSizeDialog.svelte";
  import InfoDialogs from "./InfoDialogs.svelte";
  import LayerStyleDialog from "./LayerStyleDialog.svelte";
  import NewDocumentDialog from "./NewDocumentDialog.svelte";
  import SimpleDialogs from "./SimpleDialogs.svelte";

  const needsDoc = $derived(!!pro.dialog && !["new", "shortcuts", "about", "color"].includes(pro.dialog.kind));
</script>

{#if pro.dialog && (!needsDoc || (editor.hasDocument && editor.summary))}
  {@const d = pro.dialog}
  {#key d}
    {#if d.kind === "new"}
      <NewDocumentDialog />
    {:else if d.kind === "export"}
      <ExportDialog />
    {:else if d.kind === "image-size"}
      <ImageSizeDialog />
    {:else if d.kind === "canvas-size"}
      <CanvasSizeDialog />
    {:else if d.kind === "filter"}
      <FilterDialog def={d.def} smart={d.smart} />
    {:else if d.kind === "adjustment"}
      <AdjustmentDialog adjKind={d.adjKind} initial={d.initial} />
    {:else if d.kind === "layer-style"}
      <LayerStyleDialog id={d.id} section={d.section} />
    {:else if d.kind === "shortcuts" || d.kind === "about" || d.kind === "fill-layer"}
      <InfoDialogs spec={d} />
    {:else}
      <SimpleDialogs spec={d} />
    {/if}
  {/key}
{/if}
