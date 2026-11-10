/**
 * Lookalike Domain Checker (safely.sh/tools/lookalike-domain-checker).
 *
 * 1. The visitor types a website or email address, and (optional) the
 *    real company's website, then clicks "Check name".
 * 2. The page asks /api/v1/tools/lookalike?domain=...&real=... and shows
 *    the answer: a colored verdict, the tricks found and what was compared.
 * 3. All words come from the hidden texts in the box, which the server
 *    filled in the page's language, so nothing here is in English only.
 * 4. The address keeps ?domain=...&real=..., so a result can be shared,
 *    and a shared link runs the check by itself.
 */
(function () {
  "use strict";

  const CHROME_STORE =
    "https://chromewebstore.google.com/detail/safely/bmjdfmkocaaeephfpokmpffhlogmjlnf";

  type Verdict = "same" | "lookalike" | "close" | "different" | "no_brand";

  interface Trick {
    kind: string;
    a?: string;
    b?: string;
  }

  interface Answer {
    domain: string;
    site: string;
    compared_with: string | null;
    brand: string | null;
    verdict: Verdict;
    tricks: Trick[];
  }

  const COLORS: Record<Verdict, string> = {
    same: "is-good",
    lookalike: "is-bad",
    close: "is-warn",
    different: "",
    no_brand: "",
  };

  function setup(box: HTMLElement): void {
    const formEl = box.querySelector<HTMLFormElement>("[data-ldc-form]");
    const domainEl = box.querySelector<HTMLInputElement>("input[name='domain']");
    const realEl = box.querySelector<HTMLInputElement>("input[name='real']");
    const buttonEl = box.querySelector<HTMLButtonElement>("[data-ldc-button]");
    const resultEl = box.querySelector<HTMLElement>("[data-ldc-result]");
    if (!formEl || !domainEl || !realEl || !buttonEl || !resultEl) return;
    const form: HTMLFormElement = formEl;
    const domainInput: HTMLInputElement = domainEl;
    const realInput: HTMLInputElement = realEl;
    const button: HTMLButtonElement = buttonEl;
    const result: HTMLElement = resultEl;
    const api = box.dataset.api ?? "/api/v1/tools/lookalike";
    const buttonText = button.textContent ?? "";

    const texts: Record<string, string> = {};
    box.querySelectorAll<HTMLElement>("[data-ldc-texts] [data-k]").forEach((el) => {
      texts[el.dataset.k ?? ""] = el.textContent ?? "";
    });
    const t = (key: string): string => texts[key] ?? "";
    const fill = (text: string, values: Record<string, string>): string =>
      text.replace(/\{(\w+)\}/g, (all, name: string) => values[name] ?? all);

    // "/pt-br/tools/..." links stay in the same language.
    const langPrefix = window.location.pathname.startsWith("/pt-br/") ? "/pt-br" : "";

    function row(list: HTMLElement, label: string, value: string): void {
      const dt = document.createElement("dt");
      dt.textContent = label;
      const dd = document.createElement("dd");
      dd.textContent = value;
      list.append(dt, dd);
    }

    function show(className: string, title: string, text: string): HTMLElement {
      result.className = "tool-result" + (className ? " " + className : "");
      result.replaceChildren();
      const h = document.createElement("h3");
      h.textContent = title;
      result.append(h);
      if (text) {
        const p = document.createElement("p");
        p.textContent = text;
        result.append(p);
      }
      result.hidden = false;
      return result;
    }

    function showAnswer(a: Answer): void {
      // A "close" answer with nothing to compare: letters from another
      // alphabet mixed into an ordinary name.
      const key = a.verdict === "close" && !a.compared_with ? "mixed" : a.verdict;
      const real = a.brand ?? a.compared_with ?? "";
      const values = { real };
      const card = show(COLORS[a.verdict], fill(t(key + "_title"), values), fill(t(key + "_text"), values));

      if (a.tricks.length > 0) {
        const head = document.createElement("p");
        head.className = "ldc-tricks-title";
        head.textContent = t("tricks_title");
        const list = document.createElement("ul");
        list.className = "ldc-tricks";
        a.tricks.forEach((trick) => {
          const text = t("t." + trick.kind);
          if (!text) return;
          const li = document.createElement("li");
          // For letters from another alphabet, b names the alphabet.
          const b = trick.kind === "foreign_letters" ? t("script." + (trick.b ?? "")) : (trick.b ?? "");
          li.textContent = fill(text, { a: trick.a ?? "", b });
          list.append(li);
        });
        card.append(head, list);
      }

      const facts = document.createElement("dl");
      row(facts, t("checked"), a.domain);
      if (a.site !== a.domain) row(facts, t("site"), a.site);
      if (a.compared_with && a.verdict !== "same") row(facts, t("compared"), a.compared_with);
      card.append(facts);

      // A copy is often a brand-new website: one click to check its age.
      if (a.verdict !== "same" && a.verdict !== "no_brand") {
        const age = document.createElement("p");
        age.className = "ldc-age";
        const link = document.createElement("a");
        link.href = langPrefix + "/tools/domain-age-checker?domain=" + encodeURIComponent(a.site);
        link.textContent = t("age_link");
        age.append(link);
        card.append(age);
      }

      const upsell = document.createElement("p");
      upsell.className = "tool-upsell";
      upsell.append(t("upsell") + " ");
      const link = document.createElement("a");
      link.href = CHROME_STORE;
      link.target = "_blank";
      link.rel = "noopener";
      link.textContent = t("upsell_link");
      upsell.append(link);
      card.append(upsell);
    }

    function showError(code: string): void {
      show("is-bad", t("err_" + code) || t("err_failed"), "");
    }

    /** Keeps ?domain=...&real=... in the address, so the result can be shared. */
    function remember(domain: string, real: string): void {
      const url = new URL(window.location.href);
      url.searchParams.set("domain", domain);
      if (real) url.searchParams.set("real", real);
      else url.searchParams.delete("real");
      history.replaceState(null, "", url.pathname + url.search + url.hash);
    }

    async function check(typedDomain: string, typedReal: string): Promise<void> {
      const domain = typedDomain.trim();
      const real = typedReal.trim();
      if (!domain) {
        showError("invalid_domain");
        domainInput.focus();
        return;
      }
      button.disabled = true;
      button.textContent = t("checking") || buttonText;
      try {
        const query = "?domain=" + encodeURIComponent(domain) + (real ? "&real=" + encodeURIComponent(real) : "");
        const res = await fetch(api + query, { headers: { Accept: "application/json" } });
        const data: unknown = await res.json().catch(() => null);
        if (res.ok && data && typeof data === "object" && "verdict" in data) {
          showAnswer(data as Answer);
          remember(domain, real);
        } else {
          const code =
            data && typeof data === "object" && "error" in data
              ? String((data as { error: unknown }).error)
              : "failed";
          showError(code);
          if (code === "invalid_real") realInput.focus();
        }
      } catch {
        showError("failed");
      } finally {
        button.disabled = false;
        button.textContent = buttonText;
      }
    }

    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void check(domainInput.value, realInput.value);
    });

    // A shared link like /tools/lookalike-domain-checker?domain=paypa1.com&real=paypal.com
    const params = new URLSearchParams(window.location.search);
    const shared = params.get("domain");
    if (shared) {
      domainInput.value = shared;
      realInput.value = params.get("real") ?? "";
      void check(shared, realInput.value);
    }
  }

  function init(): void {
    document.querySelectorAll<HTMLElement>("[data-ldc]").forEach(setup);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
