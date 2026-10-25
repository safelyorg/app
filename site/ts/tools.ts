/**
 * Safely "Free Tools" pages: the /tools list, each /tools/<tool> and
 * each /blog/<blog>, in every language (/pt-br/tools, /pt-br/blog/...).
 *
 * 1. Language menu: each option is the address of this page in that
 *    language (the server writes it). Picking one saves the choice with
 *    the same key as i18n.ts, so the rest of the site follows, then
 *    opens that page.
 * 2. A visitor who already chose a language on another page is taken
 *    to the same page in that language, when it exists.
 * 3. "Free Tools / Blogs" switch on the list page: "Free Tools" is
 *    shown first. /tools#blogs opens the "Blogs" side, so that side
 *    can be linked to and the back button keeps it. A yellow pill
 *    slides between the two buttons, and the side shown fades in
 *    (tools.css), so the switch feels smooth instead of jumping.
 * 4. "Show more": each list shows 6 cards, then 3 more (one row) per
 *    click. The button goes away when all are shown.
 *
 * Each tool's own box (the form and the result) has its own script,
 * loaded by the box itself. This file only runs the page around it.
 */
(function () {
  "use strict";

  const STORAGE_KEY = "safely_lang"; // same key as i18n.ts
  const STEP = 3;
  const BLOGS_HASH = "#blogs";

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

  /** The address to open, keeping the "Blogs" side when it is open. */
  function withSide(address: string): string {
    if (address.includes("#") || window.location.hash !== BLOGS_HASH) return address;
    return address + BLOGS_HASH;
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
        window.location.href = withSide(picked.value);
      });
    });

    const choice = readChoice();
    if (!choice || choice === pageLang || !selects[0]) return;
    // Only move to the same page in the chosen language. If this tool or
    // blog is not translated yet, the visitor stays on it.
    const match = selects[0].querySelector<HTMLOptionElement>(
      `option[data-lang="${choice}"][data-exact="true"]`,
    );
    if (match) {
      saveChoice(choice);
      window.location.replace(withSide(match.value));
    }
  }

  function setupTabs(): void {
    const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-tab]"));
    if (tabs.length === 0) return;
    const track = tabs[0]?.closest<HTMLElement>(".tool-tabs") ?? null;
    const slider = track?.querySelector<HTMLElement>(".tool-tab-slider") ?? null;

    /** Puts the sliding yellow pill under the chosen button. */
    function moveSlider(): void {
      const on = tabs.find((tab) => tab.getAttribute("aria-selected") === "true");
      if (!slider || !on) return;
      slider.style.width = on.offsetWidth + "px";
      slider.style.transform = "translateX(" + on.offsetLeft + "px)";
    }

    function show(name: string, focus: boolean): void {
      tabs.forEach((tab) => {
        const on = tab.dataset.tab === name;
        tab.setAttribute("aria-selected", String(on));
        tab.tabIndex = on ? 0 : -1;
        if (on && focus) tab.focus();
      });
      document.querySelectorAll<HTMLElement>("[data-panel]").forEach((panel) => {
        panel.hidden = panel.dataset.panel !== name;
      });
      moveSlider();
    }

    function sideFromAddress(): string {
      return window.location.hash === BLOGS_HASH ? "blogs" : "tools";
    }

    tabs.forEach((tab, i) => {
      tab.addEventListener("click", () => {
        const name = tab.dataset.tab ?? "tools";
        show(name, false);
        // Keep the side in the address without jumping down the page.
        const address =
          window.location.pathname + window.location.search + (name === "blogs" ? BLOGS_HASH : "");
        history.replaceState(null, "", address);
      });
      // Left and right arrow keys move between the two sides.
      tab.addEventListener("keydown", (e) => {
        if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
        e.preventDefault();
        const next = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length];
        next?.click();
        next?.focus();
      });
    });

    // The pill starts under the chosen button without sliding there.
    if (track && slider) {
      track.classList.add("has-slider", "no-anim");
    }
    show(sideFromAddress(), false);
    if (track && slider) {
      requestAnimationFrame(() =>
        requestAnimationFrame(() => track.classList.remove("no-anim")),
      );
      // Button widths change when the font loads or the window resizes.
      window.addEventListener("resize", moveSlider);
      document.fonts?.ready.then(moveSlider).catch(() => {});
    }
    window.addEventListener("hashchange", () => show(sideFromAddress(), false));
  }

  function setupShowMore(): void {
    document.querySelectorAll<HTMLButtonElement>("[data-more]").forEach((btn) => {
      const list = btn.closest("[data-list]")?.querySelector("ul.tool-cards");
      if (!list) return;
      btn.hidden = false;
      btn.addEventListener("click", () => {
        const next = Array.from(list.querySelectorAll<HTMLLIElement>("li[hidden]")).slice(0, STEP);
        next.forEach((li) => {
          li.hidden = false;
        });
        // Keyboard users land on the first new card.
        next[0]?.querySelector("a")?.focus();
        if (!list.querySelector("li[hidden]")) btn.hidden = true;
      });
    });
  }

  function init(): void {
    setupLanguage();
    setupTabs();
    setupShowMore();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
