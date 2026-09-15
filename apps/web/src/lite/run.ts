// Running features that may not exist yet: AI functions that throw
// NotReady, and engine ops another workstream has not landed. Either way the
// person sees one plain sentence, and the control remembers it is not ready.

import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import { isNotReady, isUnknownOp, lite } from "./lite.svelte";

export function comingSoon(key: string, feature: string) {
  lite.markUnavailable(key);
  editor.toast(t("{feature} is coming soon. It will run on this device when it arrives.", { feature }));
}

/** Run an AI feature. Returns true when it finished. */
export async function runFeature(key: string, feature: string, fn: () => Promise<unknown>): Promise<boolean> {
  if (lite.working) return false;
  lite.working = key;
  try {
    await fn();
    return true;
  } catch (e) {
    if (isNotReady(e) || isUnknownOp(e)) comingSoon(key, feature);
    else editor.error(e);
    return false;
  } finally {
    lite.working = null;
  }
}

/** Run an engine op that may not be implemented yet. */
export async function runOp(key: string, feature: string, cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray): Promise<boolean> {
  const r = await lite.tryExec(cmd, bytes);
  if (r.ok) return true;
  if (isUnknownOp(r.error)) comingSoon(key, feature);
  else editor.error(r.error);
  return false;
}
