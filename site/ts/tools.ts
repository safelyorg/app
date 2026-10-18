/**
 * Safely free tools pages: the /tools list and each /tools/<tool>,
 * in every language (/pt-br/tools, /pt-br/tools/<tool>).
 *
 * 1. Language menu: each option is the address of this page in that
 *    language (the server writes it). Picking one saves the choice with
 *    the same key as i18n.ts, so the rest of the site follows, then
 *    opens that page.
 * 2. A visitor who already chose a language on another page is taken
 *    to the same page in that language, when it exists.
 * 3. "Show more tools": the list shows 6 tools, then 3 more
 *    (one row) per click. The button goes away when all are shown.
 *
 * Each tool's own box (the form and the result) has its own script,
 * loaded by the box itself. This file only runs the page around it.
 */
(function () {
  "use strict";

  const STORAGE_KEY = "safely_lang"; // same key as i18n.ts
  const STEP = 3;

  function readChoice(): string | null {
    const fromLink = new URLSearchParams(window.location.search)
      .get("lang")
      ?.toLowerCase();
    if (fromLink) return fromLink === "pt" ? "pt-br" : fromLink;
    try {
      return localStorage.getItem(STORAGE_KEY);
    } catch {
      return null;
    }
  }

  function saveChoice(code: string): void {
    try {
      localStorage.setItem(STORAGE_KEY, code);
    } catch {
      // private window: the menu still works, the choice is just not kept
    }
  }

  function setupLanguage(): void {
    const pageLang = document.documentElement.dataset.toolsLang ?? "en";
    const selects = document.querySelectorAll<HTMLSelectElement>("select[data-tools-lang]");
    selects.forEach((sel) => {
      sel.setAttribute("autocomplete", "off");
      const current = sel.querySelector<HTMLOptionElement>(`option[data-lang="${pageLang}"]`);
      if (current) sel.value = current.value;
      sel.addEventListener("change", () => {
        const picked = sel.selectedOptions[0];
        if (!picked) return;
        saveChoice(picked.dataset.lang ?? "en");
        window.location.href = picked.value;
      });
    });

    const choice = readChoice();
    if (!choice || choice === pageLang || !selects[0]) return;
    // Only move to the same page in the chosen language. If this tool
    // is not translated yet, the visitor stays on it.
    const match = selects[0].querySelector<HTMLOptionElement>(
      `option[data-lang="${choice}"][data-exact="true"]`,
    );
    if (match) {
      saveChoice(choice);
      window.location.replace(match.value);
    }
  }

  function setupShowMore(): void {
    document.querySelectorAll<HTMLButtonElement>("[data-tools-more]").forEach((btn) => {
      const list = btn.closest(".tool-list")?.querySelector("ul.tool-cards");
      if (!list) return;
      btn.hidden = false;
      btn.addEventListener("click", () => {
        const next = Array.from(list.querySelectorAll<HTMLLIElement>("li[hidden]")).slice(0, STEP);
        next.forEach((li) => {
          li.hidden = false;
        });
        // Keyboard users land on the first new tool.
        next[0]?.querySelector("a")?.focus();
        if (!list.querySelector("li[hidden]")) btn.hidden = true;
      });
    });
  }

  function init(): void {
    setupLanguage();
    setupShowMore();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
