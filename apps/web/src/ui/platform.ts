// Keyboard shortcuts written once, shown and matched per platform.
//
// A shortcut spec is `Mod+Shift+Alt+<key>`: `Mod` is Cmd on macOS and Ctrl
// elsewhere, `<key>` is a letter, digit, punctuation mark or key name
// (`F6`, `Delete`, `Escape`). Matching uses `KeyboardEvent.code`, so Shift
// and Option do not change what a punctuation key is called.

export const isMac = typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);

const PUNCT_CODES: Record<string, string> = {
  "[": "BracketLeft",
  "]": "BracketRight",
  "=": "Equal",
  "+": "Equal",
  "-": "Minus",
  ";": "Semicolon",
  "'": "Quote",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
  "\\": "Backslash",
  "`": "Backquote",
};

interface Parsed {
  mod: boolean;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
  code: string;
  key: string;
}

const cache = new Map<string, Parsed>();

function parse(spec: string): Parsed {
  const hit = cache.get(spec);
  if (hit) return hit;
  // "+" is itself a key; split on "+" but keep a trailing "+".
  const parts = spec.endsWith("++") ? [...spec.slice(0, -2).split("+"), "+"] : spec.split("+");
  const key = parts.pop() ?? "";
  const set = new Set(parts.map((p) => p.toLowerCase()));
  let code: string;
  if (/^[a-z]$/i.test(key)) code = `Key${key.toUpperCase()}`;
  else if (/^[0-9]$/.test(key)) code = `Digit${key}`;
  else code = PUNCT_CODES[key] ?? key;
  const p = { mod: set.has("mod"), shift: set.has("shift"), alt: set.has("alt"), ctrl: set.has("ctrl"), code, key };
  cache.set(spec, p);
  return p;
}

/** The platform's command modifier is held (Cmd on macOS, Ctrl elsewhere). */
export function modHeld(e: { metaKey: boolean; ctrlKey: boolean }) {
  return isMac ? e.metaKey : e.ctrlKey;
}

export function matchShortcut(e: KeyboardEvent, spec: string): boolean {
  const p = parse(spec);
  if (e.code !== p.code && !(p.code === "Delete" && e.code === "Backspace")) return false;
  const mod = modHeld(e);
  if (mod !== p.mod) return false;
  if (e.shiftKey !== p.shift) return false;
  if (e.altKey !== p.alt) return false;
  // On macOS Ctrl is its own modifier; elsewhere it is Mod.
  if (isMac && e.ctrlKey !== p.ctrl) return false;
  return true;
}

const MAC_GLYPH: Record<string, string> = { mod: "⌘", shift: "⇧", alt: "⌥", ctrl: "⌃" };
const KEY_NAMES: Record<string, string> = { Delete: isMac ? "⌫" : "Del", Backspace: "⌫", Escape: "Esc", Enter: "↩", Tab: "Tab" };

/** Human form: `⇧⌘N` on macOS, `Ctrl+Shift+N` elsewhere. */
export function formatShortcut(spec: string | undefined): string {
  if (!spec) return "";
  const p = parse(spec);
  const key = KEY_NAMES[p.key] ?? (p.key.length === 1 ? p.key.toUpperCase() : p.key);
  if (isMac) {
    return `${p.ctrl ? MAC_GLYPH.ctrl : ""}${p.alt ? MAC_GLYPH.alt : ""}${p.shift ? MAC_GLYPH.shift : ""}${p.mod ? MAC_GLYPH.mod : ""}${key}`;
  }
  const mods = [p.mod || p.ctrl ? "Ctrl" : "", p.alt ? "Alt" : "", p.shift ? "Shift" : ""].filter(Boolean);
  return [...mods, key].join("+");
}

/** True when a key press belongs to a text field rather than the app. */
export function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el || !el.tagName) return false;
  if (el.isContentEditable || el.tagName === "TEXTAREA" || el.tagName === "SELECT") return true;
  if (el.tagName === "INPUT") {
    const type = (el as HTMLInputElement).type;
    return !["checkbox", "radio", "range", "button", "color"].includes(type);
  }
  return false;
}
