/**
 * Safely dashboard i18n - same engine and conventions as site/ts/i18n.ts.
 * See that file for the full explanation of data-i18n / data-i18n-attr
 * and how to add a new language.
 *
 * The dashboard shares safely.sh's domain, so it reads the SAME
 * safely_lang localStorage key the website writes - switching language
 * on the website is picked up here automatically via the storage
 * event, with no separate dashboard language picker needed.
 */
(function () {
  "use strict";

  const STORAGE_KEY = "safely_lang";

  interface SupportedLang {
    code: string;
    label: string;
  }

  const SUPPORTED_LANGS: SupportedLang[] = [
    { code: "en", label: "English" },
    { code: "pt-br", label: "Português (BR)" },
  ];

  const DEFAULT_LANG = "en";

  const originalContent = new Map<Element, string>();
  let currentDict: Record<string, string> | null = null;

  function getSavedLang(): string {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && SUPPORTED_LANGS.some((l) => l.code === saved)) return saved;
    return DEFAULT_LANG;
  }

  const originalAttrs = new Map<Element, Map<string, string>>();

  function captureOriginals(): void {
    document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
      const attrName = el.getAttribute("data-i18n-attr");
      if (attrName) {
        const current = el.getAttribute(attrName) ?? "";
        if (!originalAttrs.has(el)) originalAttrs.set(el, new Map());
        originalAttrs.get(el)!.set(attrName, current);
      } else if (!originalContent.has(el)) {
        originalContent.set(el, el.innerHTML);
      }
    });
  }

  function applyTranslations(dict: Record<string, string> | null): void {
    document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
      const key = el.getAttribute("data-i18n");
      if (!key) return;
      const attrName = el.getAttribute("data-i18n-attr");

      if (attrName) {
        const original = originalAttrs.get(el)?.get(attrName) ?? "";
        const value = dict ? dict[key] : undefined;
        el.setAttribute(attrName, value !== undefined ? value : original);
      } else {
        const original = originalContent.get(el) ?? el.innerHTML;
        const value = dict ? dict[key] : undefined;
        el.innerHTML = value !== undefined ? value : original;
      }
    });
  }

  async function loadDict(code: string): Promise<Record<string, string> | null> {
    if (code === "en") return null;
    try {
      const res = await fetch("/dashboard/locales/" + code + ".json");
      if (!res.ok) {
        console.warn("Safely i18n: locale fetch returned", res.status, "for", code);
        return null;
      }
      return await res.json();
    } catch (e) {
      console.warn("Safely i18n: failed to load locale", code, e);
      return null;
    }
  }

  async function setLanguage(code: string): Promise<void> {
    const dict = await loadDict(code);
    currentDict = dict;
    applyTranslations(dict);
    document.documentElement.setAttribute("lang", code);
  }

  // Real, public lookup for JS-generated strings (t() in shared.ts) -
  // returns the English fallback if no translation exists, so a
  // missing key never shows a blank string.
  function t(key: string, fallback: string): string {
    if (currentDict && currentDict[key] !== undefined) return currentDict[key];
    return fallback;
  }

  async function init(): Promise<void> {
    captureOriginals();
    const lang = getSavedLang();
    if (lang !== "en") {
      await setLanguage(lang);
    }
    (window as any).safelyT = t;
    window.dispatchEvent(new CustomEvent("safely-i18n-ready"));
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }

  window.addEventListener("storage", (e: StorageEvent) => {
    if (e.key === STORAGE_KEY && e.newValue) {
      setLanguage(e.newValue);
    }
  });
})();
