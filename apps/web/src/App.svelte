<script lang="ts">
  import { onMount } from "svelte";
  import { editor } from "./lib/editor.svelte";
  import { openFile, placeFile } from "./lib/io";
  import LiteApp from "./lite/LiteApp.svelte";
  import ProApp from "./pro/ProApp.svelte";
  import ProfileChooser from "./ui/ProfileChooser.svelte";
  import Toasts from "./ui/Toasts.svelte";
  import BusyOverlay from "./ui/BusyOverlay.svelte";

  let dragging = $state(false);

  /** The native server opens a file by handing the page a one-time token. */
  async function openFromToken() {
    const params = new URLSearchParams(location.search);
    const token = params.get("open");
    if (!token) return;
    history.replaceState(null, "", location.pathname + location.hash);
    try {
      const res = await fetch(`./api/file/${encodeURIComponent(token)}`);
      if (!res.ok) throw new Error(res.status === 410 ? "that file link has already been used" : `could not fetch the file (${res.status})`);
      const name = decodeURIComponent(res.headers.get("X-File-Name") ?? "Untitled");
      const blob = await res.blob();
      if (!editor.profile) editor.setProfile("pro");
      await openFile(new File([blob], name, { type: blob.type }));
    } catch (e) {
      editor.error(e);
    }
  }

  onMount(() => {
    editor
      .init()
      .then(openFromToken)
      .catch((e) => editor.error(e));
    const beforeUnload = (e: BeforeUnloadEvent) => {
      if (editor.dirty) e.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  });

  // Pro renders dark, as every professional editor does; Lite follows the OS.
  $effect(() => {
    const root = document.documentElement;
    root.classList.toggle("oa-dark", editor.profile === "pro");
    root.classList.toggle("oa-auto", editor.profile !== "pro");
  });

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const files = [...(e.dataTransfer?.files ?? [])];
    if (!files.length) return;
    if (editor.hasDocument && editor.profile === "pro") {
      for (const f of files) await placeFile(f);
    } else {
      await openFile(files[0]);
    }
  }

  async function onPaste(e: ClipboardEvent) {
    const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith("image/"));
    const file = item?.getAsFile();
    if (!file) return;
    e.preventDefault();
    if (editor.hasDocument) await placeFile(new File([file], "Pasted image.png", { type: file.type }));
    else await openFile(new File([file], "Pasted image.png", { type: file.type }));
  }
</script>

<svelte:window onpaste={onPaste} />

<div
  class="ops-app"
  role="application"
  ondragover={(e) => {
    e.preventDefault();
    dragging = true;
  }}
  ondragleave={(e) => {
    if (e.relatedTarget === null) dragging = false;
  }}
  ondrop={onDrop}
>
  {#if !editor.profile}
    <ProfileChooser />
  {:else if editor.profile === "lite"}
    <LiteApp />
  {:else}
    <ProApp />
  {/if}
  {#if dragging}
    <div class="ops-drop" aria-hidden="true"><span>Drop to open</span></div>
  {/if}
  <BusyOverlay />
  <Toasts />
</div>

<style>
  .ops-app {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .ops-drop {
    position: absolute;
    inset: 12px;
    border: 2px dashed var(--border-strong);
    border-radius: var(--radius-xl);
    background: color-mix(in oklab, var(--bg-page) 70%, transparent);
    display: grid;
    place-items: center;
    font: var(--type-ui);
    color: var(--text-strong);
    pointer-events: none;
    z-index: 50;
  }
</style>
