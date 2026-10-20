interface RiskWasmModule {
  default: () => Promise<void>;
  risk_level: (score: number) => string;
}

interface ReportSubmission {
  platform: string;
  platform_id: string | null;
  report_type: string;
  description: string | null;
  listing_url: string;
}

(async function () {
  "use strict";

  // Translates through core/i18n.ts; plain English if it isn't loaded.
  function tr(en: string, vars?: Record<string, string | number>): string {
    const i18n = (window as any).__safelyI18n;
    if (i18n) return i18n.t(en, vars);
    if (!vars) return en;
    return en.replace(/\{(\w+)\}/g, (whole, key) =>
      Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole,
    );
  }

  let wasm: RiskWasmModule;

  try {
    const wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
    wasm = await import(wasmUrl);
    await wasm.default();
  } catch (e) {
    console.warn("Safely: WASM blocked, using JS fallback");
    console.error("Safely: real WASM loading error was:", e);
    wasm = {
      default: async () => {},
      risk_level: (s: number): string => (s <= 33 ? "low" : s <= 66 ? "caution" : "high"),
    };
  }

  if (!(window as any).__safelyAddTab) return;

  const CHEVRON =
    '<svg class="safely-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>';

  // The words in the score card, in English (the keys for the
  // Portuguese text). Low risk never says "safe": no check can promise
  // that, and the buyer should stay careful.
  const LEVEL_LABEL: Record<string, string> = {
    low: "Low risk",
    caution: "Caution",
    high: "High risk",
  };
  const LEVEL_SAY: Record<string, string> = {
    low: "No serious warning signs.",
    caution: "Check these points before you pay.",
    high: "Don't pay this supplier yet.",
  };

  // Set by the backend when Safely could check too little about the
  // supplier (most checks came back empty). The score is then at least
  // Moderate, and the card says why instead of a plain "Caution".
  const NOT_ENOUGH_INFORMATION = "not_enough_information";

  function hasTooLittleInformation(pageData: any): boolean {
    return (pageData.riskFactors || []).some((f: any) => f && f.name === NOT_ENOUGH_INFORMATION);
  }

  // Values that mean "we don't know": rows with these are not shown.
  const EMPTY_VALUES = ["", "not found", "unknown", "none", "n/a"];

  function hasValue(value: unknown): boolean {
    if (value === null || value === undefined) return false;
    return EMPTY_VALUES.indexOf(String(value).trim().toLowerCase()) === -1;
  }

  function capitalizeFirst(str: string): string {
    return str ? str.charAt(0).toUpperCase() + str.slice(1) : str;
  }

  function isFlagged(type: string): boolean {
    return type !== "good" && type !== "info";
  }

  // Text before the ###CHECKLIST### marker some reasons carry.
  function plainText(text: string): string {
    const marker = text.indexOf("###CHECKLIST###");
    return marker === -1 ? text : text.slice(0, marker);
  }

  // ---- Score card ------------------------------------------------

  function buildRing(score: number): string {
    const r = 36;
    const c = 2 * Math.PI * r;
    const filled = (c * Math.max(0, Math.min(100, score))) / 100;
    return (
      '<div class="safely-ring"><svg viewBox="0 0 84 84">' +
      '<circle cx="42" cy="42" r="' +
      r +
      '" fill="none" stroke="#2e2e33" stroke-width="7"/>' +
      '<circle class="safely-ring-arc" cx="42" cy="42" r="' +
      r +
      '" fill="none" stroke="#ffb938" stroke-width="7" stroke-linecap="round" stroke-dasharray="' +
      filled.toFixed(1) +
      " " +
      c.toFixed(1) +
      '"/></svg><div class="safely-ring-number">' +
      score +
      "</div></div>"
    );
  }

  function buildScoreCard(pageData: any): string {
    const score = pageData.riskScore || 0;
    const level = wasm.risk_level(score);
    const tooLittle = hasTooLittleInformation(pageData);
    const signals = pageData.signals || [];
    const flags = signals.filter((s: any) => isFlagged(s.type)).length;

    const label = tooLittle ? tr("Not enough information") : tr(LEVEL_LABEL[level] || LEVEL_LABEL.high);
    // The same words as the Details tab ("need a look" / "look fine"),
    // and never "no warning signs" while a check needs a look.
    const say = tooLittle
      ? tr("Check this supplier carefully yourself")
      : level === "low" && flags === 0
        ? tr("No warning signs found.")
        : tr(LEVEL_SAY[level] || LEVEL_SAY.high);
    const sub =
      flags === 0
        ? tr("All {total} checks look fine.", { total: signals.length })
        : flags === 1
          ? tr("1 of {total} checks needs a look.", { total: signals.length })
          : tr("{n} of {total} checks need a look.", { n: flags, total: signals.length });

    return (
      '<div class="safely-verdict">' +
      buildRing(score) +
      '<div><div class="safely-level' +
      (level === "low" && !tooLittle ? " safely-level-calm" : "") +
      '">' +
      escapeHtml(label) +
      '</div><div class="safely-say">' +
      escapeHtml(say) +
      '</div><div class="safely-say-sub">' +
      escapeHtml(sub) +
      "</div></div></div>"
    );
  }

  // ---- What we found ---------------------------------------------

  interface Finding {
    flag: boolean;
    title: string;
    value: string;
    text: string;
  }

  const SEVERITY_ORDER = ["hard", "compound", "soft"];

  const SEVERITY_LABELS: Record<string, string> = {
    hard: "Serious",
    compound: "Pattern match",
    soft: "Worth noting",
  };

  const MAX_FINDINGS = 3;

  // Always up to three rows, problems first: the risk factors (most
  // serious first) or, without them, the flagged checks; then checks
  // that passed fill the rest. Every row has a title and a value line.
  function findings(pageData: any): Finding[] {
    const signals = pageData.signals || [];
    const factors = (pageData.riskFactors || [])
      .filter((f: any) => f && f.name !== NOT_ENOUGH_INFORMATION)
      .slice()
      .sort(
        (a: any, b: any) =>
          SEVERITY_ORDER.indexOf(a.severity) - SEVERITY_ORDER.indexOf(b.severity),
      );
    const fromSignal = (s: any): Finding => ({
      flag: isFlagged(s.type),
      title: tr(capitalizeFirst(s.label)),
      value: tr(capitalizeFirst(s.value)),
      text: capitalizeFirst(plainText(s.sub || "")),
    });
    const problems: Finding[] =
      factors.length > 0
        ? factors.map(
            (f: any): Finding => ({
              flag: true,
              title:
                f.contributing_signals && f.contributing_signals.length > 0
                  ? f.contributing_signals.map((label: string) => tr(label)).join(" + ")
                  : tr(capitalizeFirst(String(f.name || "").replace(/_/g, " "))),
              value: tr(SEVERITY_LABELS[f.severity] || "Worth noting"),
              text: capitalizeFirst(plainText(f.description || "")),
            }),
          )
        : signals.filter((s: any) => isFlagged(s.type)).map(fromSignal);
    const passed = signals.filter((s: any) => s.type === "good").map(fromSignal);
    return problems.concat(passed).slice(0, MAX_FINDINGS);
  }

  function buildFindings(pageData: any): string {
    const list = findings(pageData);
    if (list.length === 0) return "";
    const rows = list
      .map((f, i) => {
        const canOpen = !!f.text;
        const open = canOpen && i === 0 && f.flag;
        return (
          '<div class="safely-row' +
          (open ? " safely-open" : "") +
          '">' +
          (canOpen
            ? '<button type="button" class="safely-row-head" data-safely-open aria-expanded="' +
              open +
              '">'
            : '<div class="safely-row-head">') +
          '<span class="safely-mark ' +
          (f.flag ? "safely-mark-flag" : "safely-mark-ok") +
          '">' +
          (f.flag ? "!" : SAFELY_CHECK_ICON) +
          '</span><span class="safely-row-title">' +
          escapeHtml(f.title) +
          (f.value ? '<span class="safely-row-value">' + escapeHtml(f.value) + "</span>" : "") +
          "</span>" +
          (canOpen ? CHEVRON + "</button>" : "</div>") +
          (canOpen
            ? '<div class="safely-row-more"><div><div class="safely-row-text">' +
              escapeHtml(f.text) +
              "</div></div></div>"
            : "") +
          "</div>"
        );
      })
      .join("");
    return (
      '<div class="safely-heading">' +
      escapeHtml(tr("What we found")) +
      '</div><div class="safely-rows">' +
      rows +
      "</div>"
    );
  }

  // ---- Supplier --------------------------------------------------

  function fact(label: string, value: string): string {
    return (
      '<div class="safely-fact"><small>' +
      escapeHtml(label) +
      "</small><div>" +
      escapeHtml(value) +
      "</div></div>"
    );
  }

  function buildSupplier(pageData: any): string {
    const seller = pageData.seller || {};
    const platform = hasValue(seller.platform) ? String(seller.platform) : "";
    const facts: string[] = [];

    if (hasValue(seller.accountAge)) {
      facts.push(
        fact(
          platform ? tr("On {platform} for", { platform }) : tr("Account age"),
          tr(String(seller.accountAge)),
        ),
      );
    }
    facts.push(fact(tr("Reports on Safely"), String(pageData.fraudReportCount || 0)));
    if (hasValue(seller.phone)) facts.push(fact(tr("Phone"), String(seller.phone)));
    if (hasValue(seller.verification)) {
      facts.push(fact(tr("Status"), capitalizeFirst(tr(String(seller.verification).toLowerCase()))));
    }
    if (hasValue(seller.handle)) facts.push(fact(tr("Contact"), String(seller.handle)));
    if (hasValue(seller.lastActive)) facts.push(fact(tr("Last active"), tr(String(seller.lastActive))));

    const name = hasValue(seller.name) ? String(seller.name) : platform || tr("Supplier");
    const where = hasValue(seller.location) ? tr(String(seller.location)) : "";

    return (
      '<div class="safely-heading">' +
      escapeHtml(tr("Supplier")) +
      '</div><div class="safely-supplier"><div class="safely-supplier-name">' +
      escapeHtml(name) +
      "</div>" +
      (where ? '<div class="safely-supplier-where">' + escapeHtml(where) + "</div>" : "") +
      '<div class="safely-facts">' +
      facts.join("") +
      "</div>" +
      buildVisits(seller.monthlyActivity || []) +
      "</div>"
    );
  }

  // Visits by month, January to December. This month is dark, the rest
  // grey. Hidden when there were no visits at all.
  function buildVisits(months: number[]): string {
    const values = months.slice(0, 12).map((v) => Math.max(0, Number(v) || 0));
    const total = values.reduce((sum, v) => sum + v, 0);
    if (total === 0) return "";
    const max = Math.max.apply(null, values);
    const thisMonth = new Date().getMonth();
    const bars = values
      .map(
        (v, i) =>
          "<i" +
          (i === thisMonth ? ' class="safely-now-bar"' : "") +
          ' style="height:' +
          (v === 0 ? 2 : Math.max(4, Math.round((v / max) * 34))) +
          'px"></i>',
      )
      .join("");
    return (
      '<div class="safely-bars">' +
      bars +
      '</div><div class="safely-bars-label"><span>' +
      escapeHtml(tr("Safely visits by month")) +
      "</span><span>" +
      total +
      "</span></div>"
    );
  }

  // ---- Go ahead / walk away, report link, time ----------------------

  // The line shown instead of the Report link once this person has
  // reported the supplier (null: show the link).
  let reportedNote: string | null = null;

  function buildActions(pageData: any): string {
    const high = wasm.risk_level(pageData.riskScore || 0) === "high";
    return (
      '<div class="safely-outcome" id="safely-outcome-buttons">' +
      '<button type="button" class="safely-btn safely-btn-line" id="safely-outcome-proceed">' +
      escapeHtml(tr("I'll go ahead")) +
      "</button>" +
      '<button type="button" class="safely-btn ' +
      (high ? "safely-btn-dark" : "safely-btn-line") +
      '" id="safely-outcome-abort">' +
      escapeHtml(tr("I'll walk away")) +
      "</button></div>" +
      '<div class="safely-outcome-done" id="safely-outcome-confirmed">' +
      escapeHtml(tr("Thanks - your response has been recorded.")) +
      "</div>" +
      '<div class="safely-report-line">' +
      (reportedNote
        ? escapeHtml(tr(reportedNote))
        : escapeHtml(tr("Had a problem with this supplier?")) +
          ' <button type="button" class="safely-link" id="safely-report-open">' +
          escapeHtml(tr("Report it")) +
          "</button>") +
      "</div>" +
      (pageData.scanSeconds
        ? '<div class="safely-foot">' +
          escapeHtml(tr("Checked in {s} seconds", { s: pageData.scanSeconds })) +
          "</div>"
        : "")
    );
  }

  function buildRiskTab(): string {
    const pageData = (window as any).__safelyData;
    return (
      buildScoreCard(pageData) +
      buildFindings(pageData) +
      buildSupplier(pageData) +
      buildActions(pageData)
    );
  }

  // ---- Report sheet ----------------------------------------------
  // [value the server expects, words shown]. The values are the same
  // five the backend has always accepted.
  const REPORT_REASONS: [string, string][] = [
    ["no_delivery", "Took payment and never shipped"],
    ["fake_item", "Goods were fake or not as described"],
    ["wrong_item", "Sent the wrong goods"],
    ["non_responsive", "Stopped replying after payment"],
    ["scam", "Something else looked like a scam"],
  ];

  // The sheet sits on the panel itself, so it covers the whole panel
  // and is not moved by the tab's scroll.
  function buildSheet(): void {
    const panel = document.getElementById("safely-panel");
    if (!panel) return;
    const old = document.getElementById("safely-report-sheet");
    if (old) old.remove();

    const wrap = document.createElement("div");
    wrap.className = "safely-sheet-wrap";
    wrap.id = "safely-report-sheet";
    wrap.innerHTML =
      '<div class="safely-sheet-dim" data-safely-close></div>' +
      '<div class="safely-sheet" role="dialog" aria-modal="true">' +
      '<div class="safely-sheet-grab"></div>' +
      '<div id="safely-report-form">' +
      '<div class="safely-sheet-top"><div class="safely-sheet-title">' +
      escapeHtml(tr("Report this supplier")) +
      '</div><div class="safely-close-btn" data-safely-close title="' +
      escapeHtml(tr("Close")) +
      '"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg></div></div>' +
      '<div class="safely-sheet-lead">' +
      escapeHtml(tr("Your report warns other buyers who check this supplier.")) +
      "</div>" +
      REPORT_REASONS.map(
        ([value, words]) =>
          '<label class="safely-option"><input type="radio" name="safely-report-reason" value="' +
          value +
          '"><span class="safely-option-dot"></span>' +
          escapeHtml(tr(words)) +
          "</label>",
      ).join("") +
      '<button type="button" class="safely-btn safely-btn-dark safely-btn-full" id="safely-report-submit" style="margin-top:12px" disabled>' +
      escapeHtml(tr("Send report")) +
      "</button></div>" +
      '<div class="safely-sheet-done" id="safely-report-done"><b>' +
      escapeHtml(tr("Report sent")) +
      "</b>" +
      escapeHtml(tr("Thank you for warning other buyers.")) +
      "</div></div>";
    panel.appendChild(wrap);

    wrap.querySelectorAll("[data-safely-close]").forEach((el) =>
      el.addEventListener("click", (e) => {
        e.stopPropagation();
        closeSheet();
      }),
    );
    const submitBtn = wrap.querySelector("#safely-report-submit") as HTMLButtonElement;
    wrap.querySelectorAll('input[name="safely-report-reason"]').forEach((input) =>
      input.addEventListener("change", () => {
        submitBtn.disabled = false;
      }),
    );
    submitBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      sendReport(wrap, submitBtn);
    });
    if ((window as any).__safelyPreventInputBubbling) {
      (window as any).__safelyPreventInputBubbling();
    }
  }

  function openSheet(): void {
    buildSheet();
    const wrap = document.getElementById("safely-report-sheet");
    if (!wrap) return;
    void wrap.offsetWidth;
    wrap.classList.add("safely-open");
  }

  function closeSheet(): void {
    const wrap = document.getElementById("safely-report-sheet");
    if (wrap) wrap.classList.remove("safely-open");
  }

  document.addEventListener(
    "keydown",
    (e) => {
      if (e.key === "Escape") closeSheet();
    },
    true,
  );

  // Points a seller's reports add to the score - the same rule as the
  // backend (report_points in services/fraud_reports.rs): OLX 0 / 20 /
  // 35 / 50, suppliers 5 per report and 20 at most. Only used when the
  // server didn't send the new score back.
  function reportPoints(count: number, isB2b: boolean): number {
    if (isB2b) return Math.min(Math.max(count, 0), 4) * 5;
    if (count <= 0) return 0;
    if (count === 1) return 20;
    if (count === 2) return 35;
    return 50;
  }

  // The lowest score a supplier with this many reports can have - the
  // backend's report_floor: 2 reports 35, 3 reports 67, then +10 each,
  // 95 at most.
  function reportFloor(count: number): number {
    if (count <= 1) return 0;
    if (count === 2) return 35;
    if (count === 3) return 67;
    if (count === 4) return 77;
    if (count === 5) return 87;
    return 95;
  }

  async function sendReport(wrap: HTMLElement, submitBtn: HTMLButtonElement): Promise<void> {
    const selected = wrap.querySelector<HTMLInputElement>(
      'input[name="safely-report-reason"]:checked',
    );
    if (!selected) return;

    const pageData = (window as any).__safelyData;
    submitBtn.textContent = tr("Sending…");
    submitBtn.disabled = true;

    const reportData: ReportSubmission = {
      platform: pageData.seller.platform || "olx",
      platform_id: pageData.seller.platformId || null,
      report_type: selected.value,
      description: null,
      listing_url: window.location.href,
    };

    const result = await (window as any).__safelyAPI.submitReport(reportData);

    if (result && result.error === "already_reported") {
      // This person reported the supplier before: nothing changes.
      reportedNote = "You already reported this supplier.";
      pageData.reportedByYou = true;
      closeSheet();
      redrawRiskTab();
      return;
    }

    if (!result || result.error) {
      submitBtn.textContent = tr("Send report");
      submitBtn.disabled = false;
      alert(
        result && result.error === "unauthorized"
          ? tr("Please sign in again to submit a report.")
          : tr("Failed to submit report. Please try again."),
      );
      return;
    }

    const form = wrap.querySelector("#safely-report-form") as HTMLElement | null;
    const done = wrap.querySelector("#safely-report-done") as HTMLElement | null;
    if (form) form.style.display = "none";
    if (done) done.style.display = "block";

    // Show the report right away without another /analyze call - that
    // would quietly count this visit twice. The server saves the new
    // score on this scan and sends it back, so the panel and the
    // dashboard show the same number.
    const oldCount = pageData.fraudReportCount || 0;
    const newCount = oldCount + 1;
    pageData.fraudReportCount = newCount;
    pageData.seller.verification = "reported";
    if (typeof result.risk_score === "number") {
      pageData.riskScore = result.risk_score;
    } else {
      const isB2b = String(pageData.seller.platform || "").toLowerCase() !== "olx";
      const raised =
        (pageData.riskScore || 0) + reportPoints(newCount, isB2b) - reportPoints(oldCount, isB2b);
      pageData.riskScore = Math.min(100, Math.max(raised, isB2b ? reportFloor(newCount) : 0));
    }
    pageData.reportedByYou = true;
    reportedNote = "You reported this supplier.";

    setTimeout(() => {
      closeSheet();
      redrawRiskTab();
    }, 1600);
  }

  // ---- Listeners -------------------------------------------------

  function attachRiskTabListeners(): void {
    const tab = document.getElementById("safely-tab-risk");
    if (!tab) return;
    const openBtn = tab.querySelector("#safely-report-open");
    if (openBtn) {
      openBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        openSheet();
      });
    }
    attachOutcomeListeners(tab);
  }

  // "I'll go ahead" / "I'll walk away". A failed save says so, instead
  // of the click doing nothing.
  function attachOutcomeListeners(container: HTMLElement): void {
    const buttons = container.querySelector("#safely-outcome-buttons") as HTMLElement | null;
    const proceedBtn = container.querySelector("#safely-outcome-proceed") as HTMLButtonElement | null;
    const abortBtn = container.querySelector("#safely-outcome-abort") as HTMLButtonElement | null;
    const confirmed = container.querySelector("#safely-outcome-confirmed") as HTMLElement | null;

    async function handleOutcomeClick(action: "proceeded" | "aborted"): Promise<void> {
      const pageData = (window as any).__safelyData;
      if (!pageData || !pageData.analysisId) {
        console.error("Safely: no scan id for this result - the response can't be saved");
        alert(tr("Could not save your response. Please try again."));
        return;
      }
      if (proceedBtn) proceedBtn.disabled = true;
      if (abortBtn) abortBtn.disabled = true;

      const result = await (window as any).__safelyAPI.submitOutcome(pageData.analysisId, action);

      if (result === true) {
        if (buttons) buttons.style.display = "none";
        if (confirmed) confirmed.style.display = "block";
        return;
      }
      if (proceedBtn) proceedBtn.disabled = false;
      if (abortBtn) abortBtn.disabled = false;
      alert(
        result === "unauthorized"
          ? tr("Please sign in again to save your response.")
          : tr("Could not save your response. Please try again."),
      );
    }

    if (proceedBtn) proceedBtn.addEventListener("click", () => handleOutcomeClick("proceeded"));
    if (abortBtn) abortBtn.addEventListener("click", () => handleOutcomeClick("aborted"));
  }

  function redrawRiskTab(): void {
    const tabEl = document.getElementById("safely-tab-risk");
    if (tabEl) {
      tabEl.innerHTML = buildRiskTab();
      attachRiskTabListeners();
    }
  }

  // A new scan starts fresh: no report from the last page carries over,
  // but a supplier you reported before says so instead of offering the
  // Report link again.
  window.addEventListener("safely-data-ready", () => {
    const data = (window as any).__safelyData;
    reportedNote = data && data.reportedByYou ? "You already reported this supplier." : null;
    closeSheet();
    redrawRiskTab();
  });
  window.addEventListener("safely-lang-changed", () => {
    closeSheet();
    redrawRiskTab();
  });
  window.addEventListener("safely-result-text-changed", redrawRiskTab);

  (window as any).__safelyAddTab(
    "risk",
    "Risk",
    buildRiskTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
    () => attachRiskTabListeners(),
  );
})();
