/**
 * Safely i18n — standardized, minimal translation engine.
 *
 * HOW THIS WORKS:
 * - English lives directly in the HTML - it is the default and never
 *   needs its own translation file.
 * - Every other language gets ONE JSON file under /locales/<code>.json,
 *   a flat object of { "key": "translated text" }.
 * - Translatable elements are marked with data-i18n="key" in HTML.
 *   By default the engine sets that element's innerHTML - if you need
 *   to translate an attribute instead (placeholder, aria-label, etc.),
 *   add data-i18n-attr="attrName" alongside data-i18n="key".
 *
 * KEY NAMING CONVENTION (keep this consistent for every new key):
 *   "section.element" - e.g. "nav.pricing", "hero.title",
 *   "footer.tagline", "modal.about.title", "modal.about.body1".
 *   Numbered suffixes (body1, body2) for multiple paragraphs in the
 *   same block.
 *
 * TO ADD A NEW LANGUAGE LATER:
 *   1. Create /locales/<code>.json with the same keys as pt-br.json
 *   2. Add one entry to SUPPORTED_LANGS below
 *   3. Nothing else in this file, or in any HTML page, needs to change
 *      - every page's dropdown builds its own options from this list.
 */

(function () {
  "use strict";

  const STORAGE_KEY = "safely_lang";

  interface SupportedLang {
    code: string;
    label: string;
    shortLabel?: string;
  }

  const SUPPORTED_LANGS: SupportedLang[] = [
    { code: "en", label: "English", shortLabel: "EN" },
    { code: "pt-br", label: "Português (BR)", shortLabel: "PT" },
  ];

  const DEFAULT_LANG = "en";

  function getSavedLang(): string {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && SUPPORTED_LANGS.some((l) => l.code === saved)) return saved;
    return DEFAULT_LANG;
  }

  function saveLang(code: string): void {
    localStorage.setItem(STORAGE_KEY, code);
  }

  // Real, original English content - captured once, directly from the
  // HTML, before any translation is applied. This is what lets
  // switching back to English work instantly, with no reload and no
  // separate English JSON file to maintain.
  const originalContent = new Map<Element, string>();
  const originalAttrs = new Map<Element, Map<string, string>>();

  function captureOriginals(): void {
    document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
      const attrName = el.getAttribute("data-i18n-attr");
      if (attrName) {
        const current = el.getAttribute(attrName) ?? "";
        if (!originalAttrs.has(el)) originalAttrs.set(el, new Map());
        originalAttrs.get(el)!.set(attrName, current);
      } else {
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
      const res = await fetch("/locales/" + code + ".json");
      if (!res.ok) return null;
      return await res.json();
    } catch (e) {
      console.warn("Safely i18n: failed to load locale", code, e);
      return null;
    }
  }

  function syncSelects(code: string): void {
    document
      .querySelectorAll<HTMLSelectElement>("[data-safely-lang-select]")
      .forEach((sel) => {
        sel.value = code;
      });
  }

  async function setLanguage(code: string): Promise<void> {
    saveLang(code);
    const dict = await loadDict(code);
    applyTranslations(dict);
    document.documentElement.setAttribute("lang", code);
    syncSelects(code);
  }

  function buildSelectOptions(sel: HTMLSelectElement): void {
    sel.setAttribute("autocomplete", "off");
    sel.innerHTML = "";
    SUPPORTED_LANGS.forEach((lang) => {
      const opt = document.createElement("option");
      opt.value = lang.code;
      opt.textContent = lang.shortLabel ?? lang.label;
      sel.appendChild(opt);
    });
  }

  function wireSelects(): void {
    document
      .querySelectorAll<HTMLSelectElement>("[data-safely-lang-select]")
      .forEach((sel) => {
        buildSelectOptions(sel);
        sel.addEventListener("change", () => setLanguage(sel.value));
      });
  }

  async function init(): Promise<void> {
    captureOriginals();
    wireSelects();
    const lang = getSavedLang();
    syncSelects(lang);
    if (lang !== "en") {
      const dict = await loadDict(lang);
      applyTranslations(dict);
      document.documentElement.setAttribute("lang", lang);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }

  // Keeps the sign-in overlay (an iframe embedded directly in the
  // page, never itself reloaded) in sync when the language is
  // switched on the page around it. The browser fires this event in
  // every other same-origin window/iframe automatically whenever
  // localStorage changes - exactly the case here.
  window.addEventListener("storage", (e: StorageEvent) => {
    if (e.key === STORAGE_KEY && e.newValue) {
      setLanguage(e.newValue);
    }
  });
})();
