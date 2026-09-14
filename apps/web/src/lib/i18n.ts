// Translation. English text is the key (the suite's convention, see the
// openapps-i18n skill). Until locales are added this returns the English,
// with `{name}` placeholders filled from `vars`.
export function t(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
}
