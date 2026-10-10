interface RiskWasmModule {
  default: () => Promise<void>;
  risk_level: (score: number) => string;
  risk_label: (level: string) => string;
  risk_desc: (level: string) => string;
  build_activity_bars: (activity: Uint8Array) => string;
  verification_badge: (status: string) => string;
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

  function isPortuguese(): boolean {
    const i18n = (window as any).__safelyI18n;
    return !!i18n && i18n.getLang() === "pt-br";
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
      risk_label: (l: string): string =>
        l === "low" ? "Low risk" : l === "caution" ? "Caution" : "High risk",
      risk_desc: (l: string): string =>
        l === "low"
          ? "No major warnings found"
          : l === "caution"
            ? "Review before proceeding"
            : "High risk detected",
      build_activity_bars: (a: Uint8Array): string => {
        const values = Array.from(a);
        const rawMax = values.length > 0 ? Math.max(...values) : 1;
        const max = rawMax === 0 ? 1 : rawMax;

        return values
          .map((v) => {
            const pct = Math.round((v / max) * 100);
            const heightPx = Math.max(2, Math.round((pct / 100) * 44));
            const opacity = v === 0 ? 0.15 : 0.3 + (pct / 100) * 0.7;

            return (
              '<div style="flex:1;display:flex;flex-direction:column;justify-content:flex-end;align-items:center;height:100%;position:relative;">' +
              '<div class="safely-activity-bar" style="height:' +
              heightPx +
              "px;opacity:" +
              opacity.toFixed(2) +
              ';width:100%;"></div>' +
              '<span style="position:absolute;top:6px;font-size:9px;color:#ffffff;line-height:1;">' +
              v +
              "</span></div>"
            );
          })
          .join("");
      },
      verification_badge: (s: string): string =>
        '<span class="safely-verified-badge">' + escapeHtml(s) + "</span>",
    };
  }

  if (!(window as any).__safelyAddTab) return;

  let currentRiskSubTab: "seller" | "report" = "seller";

  // Same palette as the dashboard's RISK_HEX map.
  const RISK_HEX: Record<string, string> = {
    low: "#35d0a6",
    caution: "#f2b84c",
    high: "#ff5d5d",
  };

  // The words under the score for each risk level, in English (the
  // keys for the Portuguese text). Same words as risk_label / risk_desc
  // in wasm/src/lib.rs. Low risk never says "safe": no check can
  // promise that, and the buyer should stay careful.
  const LEVEL_LABEL: Record<string, string> = {
    low: "Low risk",
    caution: "Caution",
    high: "High risk",
  };
  const LEVEL_DESC: Record<string, string> = {
    low: "No major warnings found",
    caution: "Review before proceeding",
    high: "High risk detected",
  };

  // Set by the backend when Safely could check too little about the
  // supplier (most checks came back empty). The score is then at least
  // Moderate, and the label says why instead of a plain "Caution".
  const NOT_ENOUGH_INFORMATION = "not_enough_information";

  function hasTooLittleInformation(pageData: any): boolean {
    return (pageData.riskFactors || []).some((f: any) => f && f.name === NOT_ENOUGH_INFORMATION);
  }

  function riskLabelFor(level: string, pageData: any): string {
    if (hasTooLittleInformation(pageData)) return tr("Not enough information");
    return tr(LEVEL_LABEL[level] || LEVEL_LABEL.high);
  }

  function riskDescFor(level: string, pageData: any): string {
    if (hasTooLittleInformation(pageData)) return tr("Check this supplier carefully yourself");
    return tr(LEVEL_DESC[level] || LEVEL_DESC.high);
  }

  // Labels only the backend's B2B (supplier) scans produce.
  const B2B_ONLY_LABELS = [
    "Company profile completeness",
    "Listing completeness",
    "Registration consistency",
  ];

  // What to check before paying, shown at the top of a Moderate result:
  // Moderate covers a wide range (34-66) and buyers may read it as
  // "fine". High results already say "High risk detected".
  const B2B_BEFORE_YOU_PAY = [
    "Order a sample first",
    "Pay through the platform's buyer protection, or only to a bank account in the company's own name",
    "Confirm the bank details on a video call",
  ];
  const B2C_BEFORE_YOU_PAY = [
    "Ask for a live video call",
    "Do not pay to number in listing",
    "Don't pay the full amount before delivery",
  ];

  function beforeYouPayHTML(level: string, pageData: any): string {
    if (level !== "caution") return "";
    const isB2b = (pageData.signals || []).some((s: any) => B2B_ONLY_LABELS.includes(s.label));
    const items = (isB2b ? B2B_BEFORE_YOU_PAY : B2C_BEFORE_YOU_PAY)
      .map(
        (item) =>
          '<div style="display:flex;gap:8px;align-items:flex-start;margin-top:6px;">' +
          '<span style="color:#f2b84c;flex-shrink:0;">&#10003;</span>' +
          "<span>" +
          escapeHtml(tr(item)) +
          "</span></div>",
      )
      .join("");
    return (
      '<div class="safely-network-alert safely-alert-caution" style="display:block;margin:0 0 14px;font-size:12px;line-height:1.45;">' +
      '<div style="font-weight:700;">' +
      escapeHtml(tr("Before you pay, check:")) +
      "</div>" +
      items +
      "</div>"
    );
  }

  // The status badge comes from WASM in English; in Portuguese only its
  // visible word is swapped, so its colour and style stay the same.
  function verificationBadgeFor(status: string): string {
    const html = wasm.verification_badge(status);
    if (!isPortuguese() || !status) return html;
    const word = status.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return html.replace(new RegExp(">\\s*" + word + "\\s*<", "i"), ">" + escapeHtml(tr(status)) + "<");
  }

  // The line under the activity chart. In Portuguese it is written
  // here from the report count; in English the server's sentence is
  // shown as before.
  function networkSummaryFor(pageData: any): string {
    if (!isPortuguese()) return pageData.seller.networkSummary;
    const count = pageData.fraudReportCount || 0;
    if (count === 0) return tr("No fraud reports found on the Safely network.");
    if (count === 1) return tr("1 fraud report found on the Safely network. Proceed with caution.");
    return tr("{n} fraud reports found on the Safely network. High-risk seller.", { n: count });
  }

  // A seller detail ("Not found", "About 11 years", a name...) in the
  // current language, made safe to put in the page.
  function detail(value: unknown): string {
    return escapeHtml(tr(value === null || value === undefined ? "" : String(value)));
  }

  function buildRiskGauge(score: number, level: string): string {
    const color = RISK_HEX[level] || RISK_HEX.high;
    const r = 44;
    const circumference = 2 * Math.PI * r;
    const offset = circumference * (1 - score / 100);
    let ticks = "";
    const tickCount = 40;

    for (let i = 0; i < tickCount; i++) {
      const angle = (i * 360) / tickCount;
      const major = i % 5 === 0;
      const len = major ? 6 : 3;
      const outerR = 54;
      const innerR = outerR - len;
      ticks +=
        '<line x1="60" y1="' +
        (60 - outerR) +
        '" x2="60" y2="' +
        (60 - innerR) +
        '" stroke="' +
        (major ? "#3a3a42" : "#24242b") +
        '" stroke-width="1.5" transform="rotate(' +
        angle +
        ' 60 60)" />';
    }

    return (
      '<svg viewBox="0 0 120 120" style="width:100%;height:100%">' +
      ticks +
      '<circle cx="60" cy="60" r="' +
      r +
      '" fill="none" stroke="#1b1b20" stroke-width="9" />' +
      '<circle cx="60" cy="60" r="' +
      r +
      '" fill="none" stroke="' +
      color +
      '" stroke-width="9" stroke-linecap="round" stroke-dasharray="' +
      circumference +
      '" stroke-dashoffset="' +
      offset +
      '" transform="rotate(-90 60 60)" />' +
      '<text x="60" y="57" text-anchor="middle" font-family="JetBrains Mono, monospace" font-weight="700" font-size="30" fill="' +
      color +
      '">' +
      score +
      "</text>" +
      '<text x="60" y="75" text-anchor="middle" font-family="Inter, sans-serif" font-weight="600" font-size="9" fill="#8a8a93" letter-spacing="0.5">/ 100</text>' +
      "</svg>"
    );
  }

  function buildSellerSection(): string {
    const pageData = (window as any).__safelyData;
    const score = pageData.riskScore || 0;
    const lvl = wasm.risk_level(score);
    const riskLabel = riskLabelFor(lvl, pageData);
    const riskDesc = riskDescFor(lvl, pageData);
    const riskColor = RISK_HEX[lvl] || RISK_HEX.high;
    const activityBars = wasm.build_activity_bars(
      new Uint8Array(
        pageData.seller.monthlyActivity.map((v: number) => Math.min(255, Math.max(0, v))),
      ),
    );

    const circleHTML =
      '<div style="text-align:center;padding:20px 16px 10px">' +
      '<div style="width:120px;height:120px;margin:0 auto 12px">' +
      buildRiskGauge(score, lvl) +
      "</div>" +
      '<div style="font-size:18px;font-weight:700;color:' +
      riskColor +
      '">' +
      riskLabel +
      "</div>" +
      '<div style="font-size:13px;color:#8a8a93;margin-top:4px">' +
      riskDesc +
      "</div>" +
      "</div>";

    const sellerCardHTML =
      '<div class="safely-section-label">' +
      tr("Seller Information") +
      '</div><div class="safely-seller-card"><div class="safely-seller-name">' +
      detail(pageData.seller.name) +
      '</div><div class="safely-seller-detail"><span>' +
      tr("Username") +
      "</span><span>" +
      detail(pageData.seller.handle) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Phone") +
      "</span><span>" +
      detail(pageData.seller.phone) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Account age") +
      "</span><span>" +
      detail(pageData.seller.accountAge) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Location") +
      "</span><span>" +
      detail(pageData.seller.location) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Last active") +
      "</span><span>" +
      detail(pageData.seller.lastActive) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Status") +
      "</span>" +
      verificationBadgeFor(pageData.seller.verification) +
      '</div><div class="safely-seller-detail"><span>' +
      tr("Fraud Reports") +
      '</span><span style="color:' +
      (pageData.fraudReportCount > 0 ? "#ff5d5d" : "#8a8a93") +
      '">' +
      (pageData.fraudReportCount || 0) +
      '</span></div><div class="safely-seller-detail"><span>' +
      tr("Platform") +
      '</span><span style="text-transform:capitalize">' +
      detail(pageData.seller.platform) +
      "</span></div></div>";

    const activityHTML =
      '<div class="safely-section-label" style="margin-top:18px">' +
      tr("Visit activity — 12 months") +
      "</div>" +
      '<div class="safely-activity-card">' +
      '<div style="display:flex;align-items:flex-end;gap:3px;height:56px">' +
      activityBars +
      "</div>" +
      '<div style="display:flex;gap:3px;margin-top:4px">' +
      (function (): string {
        const months = [
          "Jan", "Feb", "Mar", "Apr", "May", "Jun",
          "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        return months
          .map(
            (m) =>
              '<span style="flex:1;text-align:center;font-size:8px;color:#8a8a93;">' +
              tr(m) +
              "</span>",
          )
          .join("");
      })() +
      "</div></div>";

    const networkHTML =
      '<div class="safely-network-alert safely-alert-' +
      lvl +
      '" style="margin-top:14px"><span>&#9679;</span><span>' +
      networkSummaryFor(pageData) +
      "</span></div>";

    const outcomeButtonsHTML =
      '<div style="display:flex;gap:8px;margin-top:14px;">' +
      '<button id="safely-outcome-proceed" style="flex:1;padding:10px;border-radius:8px;border:1px solid #35d0a6;background:transparent;color:#35d0a6;font-size:12px;font-weight:600;cursor:pointer;">' +
      escapeHtml(tr("I'm proceeding")) +
      "</button>" +
      '<button id="safely-outcome-abort" style="flex:1;padding:10px;border-radius:8px;border:1px solid #ff5d5d;background:transparent;color:#ff5d5d;font-size:12px;font-weight:600;cursor:pointer;">' +
      escapeHtml(tr("I'm backing out")) +
      "</button>" +
      "</div>" +
      '<div id="safely-outcome-confirmed" style="display:none;text-align:center;margin-top:10px;font-size:12px;color:#8a8a93;">' +
      tr("Thanks - your response has been recorded.") +
      "</div>";

    return (
      circleHTML +
      beforeYouPayHTML(lvl, pageData) +
      sellerCardHTML +
      activityHTML +
      networkHTML +
      outcomeButtonsHTML
    );
  }

  // The report reasons: [value sent to the server, name, description].
  const REPORT_REASONS: [string, string, string][] = [
    ["scam", "Scam", "Seller took payment and disappeared"],
    ["fake_item", "Fake item", "Item was counterfeit or misrepresented"],
    ["no_delivery", "No delivery", "Payment sent but item never arrived"],
    ["wrong_item", "Wrong item", "Received something different"],
    ["non_responsive", "Non responsive", "Seller stopped responding after payment"],
  ];

  function buildReportSection(): string {
    const reasons = REPORT_REASONS.map(
      ([value, name, desc]) =>
        '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="' +
        value +
        '"><div class="safely-report-reason-text"><span class="safely-report-reason-name">' +
        tr(name) +
        '</span><span class="safely-reason-desc">' +
        tr(desc) +
        "</span></div></label>",
    ).join("");
    return (
      '<div class="safely-report-section">' +
      '<div class="safely-section-label">' +
      tr("Report this seller") +
      "</div>" +
      '<p class="safely-report-desc">' +
      tr(
        "If you experienced fraud or suspicious behavior from this seller, help protect others by submitting a report.",
      ) +
      "</p>" +
      '<div class="safely-section-label" style="margin-top:14px">' +
      tr("Select reason") +
      "</div>" +
      '<div class="safely-report-reasons" id="safely-report-reasons">' +
      reasons +
      "</div>" +
      '<button class="safely-report-btn" id="safely-report-submit">' +
      tr("Submit Report") +
      "</button>" +
      '<div class="safely-report-success" id="safely-report-success" style="display:none">' +
      "<span>&#10003;</span> " +
      tr("Report submitted. Thank you for helping protect the community.") +
      "</div>" +
      "</div>"
    );
  }

  function buildRiskTab(): string {
    const sellerVisible = currentRiskSubTab === "seller";
    return (
      '<div class="safely-sub-tabs">' +
      '<button class="safely-sub-tab' +
      (sellerVisible ? " safely-active" : "") +
      '" id="safely-risk-subtab-seller">' +
      tr("Risk") +
      "</button>" +
      '<button class="safely-sub-tab' +
      (!sellerVisible ? " safely-active" : "") +
      '" id="safely-risk-subtab-report">' +
      tr("Report") +
      "</button>" +
      "</div>" +
      '<div id="safely-risk-seller-content"' +
      (sellerVisible ? "" : ' style="display:none"') +
      ">" +
      buildSellerSection() +
      "</div>" +
      '<div id="safely-risk-report-content"' +
      (!sellerVisible ? "" : ' style="display:none"') +
      ">" +
      buildReportSection() +
      "</div>"
    );
  }

  function fraudCountContribution(count: number): number {
    if (count === 0) return 0;
    if (count === 1) return 20;
    if (count === 2) return 35;
    return 50;
  }

  function attachRiskTabListeners(): void {
    const root = document.getElementById("safely-tab-risk");
    if (!root) return;

    const sellerBtn = root.querySelector("#safely-risk-subtab-seller") as HTMLElement | null;
    const reportBtn = root.querySelector("#safely-risk-subtab-report") as HTMLElement | null;
    const sellerContent = root.querySelector("#safely-risk-seller-content") as HTMLElement | null;
    const reportContent = root.querySelector("#safely-risk-report-content") as HTMLElement | null;

    if (sellerBtn && reportBtn && sellerContent && reportContent) {
      sellerBtn.addEventListener("click", () => {
        currentRiskSubTab = "seller";
        sellerBtn.classList.add("safely-active");
        reportBtn.classList.remove("safely-active");
        sellerContent.style.display = "";
        reportContent.style.display = "none";
      });

      reportBtn.addEventListener("click", () => {
        currentRiskSubTab = "report";
        reportBtn.classList.add("safely-active");
        sellerBtn.classList.remove("safely-active");
        reportContent.style.display = "";
        sellerContent.style.display = "none";
      });
    }

    const submitBtn = root.querySelector("#safely-report-submit") as HTMLButtonElement | null;
    if (submitBtn) {
      submitBtn.addEventListener("click", async () => {
        const selected = root.querySelector<HTMLInputElement>(
          'input[name="safely-report-reason"]:checked',
        );
        if (!selected) {
          alert(tr("Please select a reason before submitting."));
          return;
        }

        const pageData = (window as any).__safelyData;
        submitBtn.textContent = tr("Submitting...");
        submitBtn.disabled = true;

        const reportData: ReportSubmission = {
          platform: pageData.seller.platform || "olx",
          platform_id: pageData.seller.platformId || null,
          report_type: selected.value,
          description: null,
          listing_url: window.location.href,
        };

        const result = await (window as any).__safelyAPI.submitReport(reportData);

        if (!result || result.error) {
          submitBtn.textContent = tr("Submit Report");
          submitBtn.disabled = false;
          if (result && result.error === "unauthorized") {
            alert(tr("Please sign in again to submit a report."));
          } else {
            alert(tr("Failed to submit report. Please try again."));
          }
          return;
        }

        const success = root.querySelector("#safely-report-success") as HTMLElement | null;
        if (success) success.style.display = "flex";
        submitBtn.style.display = "none";

        // Reflect the report immediately without another /analyze
        // call - re-fetching here would quietly double-count this
        // visit's monthly activity. Updating the already-loaded data
        // in place and redrawing only the seller section avoids that.
        //
        // Mirrors the exact fraud-count contribution used by the
        // backend's calculate_risk_score - a step function, not a
        // flat +N per report.
        const oldCount = (window as any).__safelyData.fraudReportCount || 0;
        const newCount = oldCount + 1;
        const delta = fraudCountContribution(newCount) - fraudCountContribution(oldCount);

        (window as any).__safelyData.fraudReportCount = newCount;
        (window as any).__safelyData.seller.verification = "reported";
        (window as any).__safelyData.riskScore = Math.min(
          100,
          ((window as any).__safelyData.riskScore || 0) + delta,
        );

        // The network-alert sentence is plain text from the last
        // analyze call - swap in the new count wherever a standalone
        // number appears in it. (In Portuguese the sentence is written
        // from the count itself, so it is always right.)
        if ((window as any).__safelyData.seller.networkSummary) {
          (window as any).__safelyData.seller.networkSummary = (
            window as any
          ).__safelyData.seller.networkSummary.replace(/\d+/, String(newCount));
        }

        const sellerContentEl = document.getElementById("safely-risk-seller-content");
        if (sellerContentEl) sellerContentEl.innerHTML = buildSellerSection();
      });
    }

    const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement | null;
    const abortBtn = root.querySelector("#safely-outcome-abort") as HTMLButtonElement | null;
    const outcomeConfirmed = root.querySelector("#safely-outcome-confirmed") as HTMLElement | null;

    async function handleOutcomeClick(action: "proceeded" | "aborted"): Promise<void> {
      const pageData = (window as any).__safelyData;
      if (!pageData.analysisId) return;

      if (proceedBtn) proceedBtn.disabled = true;
      if (abortBtn) abortBtn.disabled = true;

      const success = await (window as any).__safelyAPI.submitOutcome(pageData.analysisId, action);

      if (success) {
        if (proceedBtn) proceedBtn.style.display = "none";
        if (abortBtn) abortBtn.style.display = "none";
        if (outcomeConfirmed) outcomeConfirmed.style.display = "block";
      } else {
        if (proceedBtn) proceedBtn.disabled = false;
        if (abortBtn) abortBtn.disabled = false;
      }
    }

    if (proceedBtn) {
      proceedBtn.addEventListener("click", () => handleOutcomeClick("proceeded"));
    }
    if (abortBtn) {
      abortBtn.addEventListener("click", () => handleOutcomeClick("aborted"));
    }
  }

  function redrawRiskTab(): void {
    const tabEl = document.getElementById("safely-tab-risk");
    if (tabEl) {
      tabEl.innerHTML = buildRiskTab();
      attachRiskTabListeners();
    }
  }

  (window as any).__safelyAddTab(
    "risk",
    "Risk",
    buildRiskTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8a8a93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
    () => {
      if ((window as any).__safelyPreventInputBubbling) {
        (window as any).__safelyPreventInputBubbling();
      }
      attachRiskTabListeners();
    },
  );

  window.addEventListener("safely-data-ready", redrawRiskTab);
  window.addEventListener("safely-lang-changed", redrawRiskTab);
  window.addEventListener("safely-result-text-changed", redrawRiskTab);
})();
