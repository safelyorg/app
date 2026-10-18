/**
 * Domain Age Checker (safely.sh/tools/domain-age-checker).
 *
 * 1. The visitor types a website and clicks "Check age".
 * 2. The page asks /api/v1/tools/domain-age?domain=... and shows the
 *    answer: a colored verdict, the dates and the registrar.
 * 3. All words come from the hidden texts in the box, which the server
 *    filled in the page's language, so nothing here is in English only.
 * 4. The address keeps ?domain=..., so a result can be shared, and a
 *    shared link runs the check by itself.
 */
(function () {
  "use strict";

  const CHROME_STORE =
    "https://chromewebstore.google.com/detail/safely/bmjdfmkocaaeephfpokmpffhlogmjlnf";

  type Verdict = "new" | "young" | "established" | "unknown";

  interface DomainAge {
    domain: string;
    registered_on: string | null;
    expires_on: string | null;
    updated_on: string | null;
    registrar: string | null;
    age_days: number | null;
    verdict: Verdict;
    expires_soon: boolean;
  }

  const ERRORS: Record<string, string> = {
    invalid_domain: "err_invalid",
    not_found: "err_not_found",
    too_many: "err_busy",
    lookup_failed: "err_failed",
  };

  const COLORS: Record<Verdict, string> = {
    new: "is-bad",
    young: "is-warn",
    established: "is-good",
    unknown: "",
  };

  function setup(box: HTMLElement): void {
    const formEl = box.querySelector<HTMLFormElement>("[data-dac-form]");
    const inputEl = box.querySelector<HTMLInputElement>("input[name='domain']");
    const buttonEl = box.querySelector<HTMLButtonElement>("[data-dac-button]");
    const resultEl = box.querySelector<HTMLElement>("[data-dac-result]");
    if (!formEl || !inputEl || !buttonEl || !resultEl) return;
    const form: HTMLFormElement = formEl;
    const input: HTMLInputElement = inputEl;
    const button: HTMLButtonElement = buttonEl;
    const result: HTMLElement = resultEl;
    const api = box.dataset.api ?? "/api/v1/tools/domain-age";
    const buttonText = button.textContent ?? "";

    const texts: Record<string, string> = {};
    box.querySelectorAll<HTMLElement>("[data-dac-texts] [data-k]").forEach((el) => {
      texts[el.dataset.k ?? ""] = el.textContent ?? "";
    });
    const t = (key: string): string => texts[key] ?? "";

    const lang = document.documentElement.lang || "en";
    const dateFormat = new Intl.DateTimeFormat(lang, {
      year: "numeric",
      month: "long",
      day: "numeric",
      timeZone: "UTC",
    });
    const showDate = (day: string): string => dateFormat.format(new Date(day + "T00:00:00Z"));

    /** 1300 days -> "3 years and 6 months", in the page's language. */
    function ageText(days: number): string {
      const count = (n: number, one: string, many: string): string =>
        n === 1 ? t(one) : t(many).replace("{n}", String(n));
      const years = Math.floor(days / 365.25);
      const months = Math.floor((days - years * 365.25) / 30.44);
      if (years > 0) {
        const y = count(years, "year", "years");
        return months > 0 ? `${y} ${t("and")} ${count(months, "month", "months")}` : y;
      }
      if (months > 0) return count(months, "month", "months");
      return count(days, "day", "days");
    }

    function row(list: HTMLElement, label: string, value: string, note?: string): void {
      const dt = document.createElement("dt");
      dt.textContent = label;
      const dd = document.createElement("dd");
      dd.textContent = value;
      if (note) {
        const small = document.createElement("span");
        small.className = "dac-note";
        small.textContent = note;
        dd.append(small);
      }
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

    function showAnswer(a: DomainAge): void {
      const age = a.age_days === null ? "" : ageText(a.age_days);
      const card = show(
        COLORS[a.verdict],
        t(a.verdict + "_title").replace("{age}", age),
        t(a.verdict + "_text"),
      );
      const list = document.createElement("dl");
      row(list, t("domain"), a.domain);
      row(list, t("registered"), a.registered_on ? showDate(a.registered_on) : t("not_shared"));
      if (a.expires_on) {
        row(list, t("expires"), showDate(a.expires_on), a.expires_soon ? t("expires_soon") : "");
      }
      if (a.updated_on) row(list, t("updated"), showDate(a.updated_on));
      if (a.registrar) row(list, t("registrar"), a.registrar);
      card.append(list);

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
      show("is-bad", t(ERRORS[code] ?? "err_failed"), "");
    }

    /** Keeps ?domain=... in the address, so the result can be shared. */
    function remember(domain: string): void {
      const url = new URL(window.location.href);
      url.searchParams.set("domain", domain);
      history.replaceState(null, "", url.pathname + url.search + url.hash);
    }

    async function check(typed: string): Promise<void> {
      const domain = typed.trim();
      if (!domain) {
        showError("invalid_domain");
        input.focus();
        return;
      }
      button.disabled = true;
      button.textContent = t("checking") || buttonText;
      try {
        const res = await fetch(api + "?domain=" + encodeURIComponent(domain), {
          headers: { Accept: "application/json" },
        });
        const data: unknown = await res.json().catch(() => null);
        if (res.ok && data && typeof data === "object" && "verdict" in data) {
          const answer = data as DomainAge;
          showAnswer(answer);
          remember(answer.domain);
        } else {
          const code =
            data && typeof data === "object" && "error" in data
              ? String((data as { error: unknown }).error)
              : "lookup_failed";
          showError(code);
        }
      } catch {
        showError("lookup_failed");
      } finally {
        button.disabled = false;
        button.textContent = buttonText;
      }
    }

    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void check(input.value);
    });

    // A shared link like /tools/domain-age-checker?domain=supplier.com
    const shared = new URLSearchParams(window.location.search).get("domain");
    if (shared) {
      input.value = shared;
      void check(shared);
    }
  }

  function init(): void {
    document.querySelectorAll<HTMLElement>("[data-dac]").forEach(setup);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
