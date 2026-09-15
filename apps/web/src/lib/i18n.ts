// Re-exported so existing `import { t } from "./lib/i18n"` call sites keep
// working; the runtime lives in `i18n/index.svelte.ts` (runes need that
// extension).
export { t, setLocale, getLocale, initLocale, missing, LOCALES, DEFAULT_LOCALE } from "./i18n/index.svelte";
