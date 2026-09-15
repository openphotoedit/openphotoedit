<script lang="ts">
  // Wraps any content in an instant tooltip. For buttons prefer the
  // `tooltip` action directly (IconButton does).
  import type { Snippet } from "svelte";
  import { tooltip } from "./tooltip";

  let { text, shortcut, placement = "bottom", children }: { text: string; shortcut?: string; placement?: "bottom" | "right" | "top" | "left"; children: Snippet } = $props();
</script>

<span class="ops-tip" use:tooltip={{ text, shortcut, placement }}>{@render children()}</span>

<style>
  .ops-tip {
    display: inline-flex;
    min-width: 0;
  }
</style>
