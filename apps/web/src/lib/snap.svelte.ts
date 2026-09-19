// View › Snap: one reactive switch the menu writes and the snapping tools
// read. It shares the "ops.snap" storage key ("on"/"off") with
// tools/snap.ts, and mirrors every change into that module's `snapConfig`
// when it exists, so tools that read either agree.

const KEY = "ops.snap";

function load(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export const snap = $state({ enabled: load() });

// Loaded lazily and optionally: the tools workstream owns tools/snap.ts.
const toolSnap = import.meta.glob("../tools/snap.ts");

async function mirror(on: boolean) {
  const loader = Object.values(toolSnap)[0];
  if (!loader) return;
  try {
    const mod = (await loader()) as { setSnapEnabled?: (on: boolean) => void; snapConfig?: { enabled: boolean } };
    if (mod.setSnapEnabled) mod.setSnapEnabled(on);
    else if (mod.snapConfig) mod.snapConfig.enabled = on;
  } catch {
    /* the snapping module failed to load; the switch still works here */
  }
}

export function setSnap(on: boolean) {
  snap.enabled = on;
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    /* private mode: lasts for this session */
  }
  void mirror(on);
}

export function toggleSnap() {
  setSnap(!snap.enabled);
}
