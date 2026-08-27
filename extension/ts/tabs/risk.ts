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

  let wasm: RiskWasmModule;

  try {
    const wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
    wasm = await import(wasmUrl);
    await wasm.default();
  } catch (e) {
    console.warn("Safely: WASM blocked, using JS fallback");
    wasm = {
      default: async () => {},
      risk_level: (s: number): string => (s <= 33 ? "low" : s <= 66 ? "caution" : "high"),
      risk_label: (l: string): string =>
        l === "low" ? "Low risk" : l === "caution" ? "Caution" : "High risk",
      risk_desc: (l: string): string =>
        l === "low"
          ? "Safe to proceed"
          : l === "caution"
            ? "Review before proceeding"
            : "High risk detected",
      build_activity_bars: (a: Uint8Array): string => {
        const max = Math.max(...Array.from(a)) || 1;
        return Array.from(a)
          .map((v) => {
            const heightPx = Math.max(3, Math.round((v / max) * 44));
            return (
              '<div style="flex:1;display:flex;flex-direction:column;align-items:center;justify-content:flex-end;position:relative;">' +
              '<span style="font-size:9px;color:#f2f1ed;font-weight:700;position:absolute;top:0;">' +
              v +
              "</span>" +
              '<div style="width:100%;background:rgba(111,179,239,0.7);border-radius:4px 4px 0 0;height:' +
              heightPx +
              'px"></div></div>'
            );
          })
          .join("");
      },
      verification_badge: (s: string): string =>
        '<span class="safely-verified-badge">' + s + "</span>",
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
    const riskLabel = wasm.risk_label(lvl);
    const riskDesc = wasm.risk_desc(lvl);
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
      '<div class="safely-section-label">Seller Information</div><div class="safely-seller-card"><div class="safely-seller-name">' +
      (pageData.seller.name || "Unknown") +
      '</div><div class="safely-seller-handle">' +
      (pageData.seller.handle || "") +
      '</div><div class="safely-seller-detail"><span>Account age</span><span>' +
      pageData.seller.accountAge +
      '</span></div><div class="safely-seller-detail"><span>Location</span><span>' +
      (pageData.seller.location || "Unknown") +
      '</span></div><div class="safely-seller-detail"><span>Last active</span><span>' +
      pageData.seller.lastActive +
      '</span></div><div class="safely-seller-detail"><span>Status</span>' +
      wasm.verification_badge(pageData.seller.verification) +
      '</div><div class="safely-seller-detail"><span>Fraud Reports</span><span style="color:' +
      (pageData.fraudReportCount > 0 ? "#ff5d5d" : "#8a8a93") +
      '">' +
      (pageData.fraudReportCount || 0) +
      '</span></div><div class="safely-seller-detail"><span>Platform</span><span style="text-transform:capitalize">' +
      (pageData.seller.platform || "Unknown") +
      "</span></div></div>";

    const activityHTML =
      '<div class="safely-section-label" style="margin-top:18px">Visit activity \u2014 12 months</div>' +
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
              '<span style="flex:1;text-align:center;font-size:8px;color:#8a8a93;">' + m + "</span>",
          )
          .join("");
      })() +
      "</div></div>";

    const networkHTML =
      '<div class="safely-network-alert safely-alert-' +
      lvl +
      '" style="margin-top:14px"><span>&#9679;</span><span>' +
      pageData.seller.networkSummary +
      "</span></div>";

    return circleHTML + sellerCardHTML + activityHTML + networkHTML;
  }

  function buildReportSection(): string {
    return (
      '<div class="safely-report-section">' +
      '<div class="safely-section-label">Report this seller</div>' +
      '<p class="safely-report-desc">If you experienced fraud or suspicious behavior from this seller, help protect others by submitting a report.</p>' +
      '<div class="safely-section-label" style="margin-top:14px">Select reason</div>' +
      '<div class="safely-report-reasons" id="safely-report-reasons">' +
      '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="scam"><div class="safely-report-reason-text"><span class="safely-report-reason-name">Scam</span><span class="safely-reason-desc">Seller took payment and disappeared</span></div></label>' +
      '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="fake_item"><div class="safely-report-reason-text"><span class="safely-report-reason-name">Fake item</span><span class="safely-reason-desc">Item was counterfeit or misrepresented</span></div></label>' +
      '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="no_delivery"><div class="safely-report-reason-text"><span class="safely-report-reason-name">No delivery</span><span class="safely-reason-desc">Payment sent but item never arrived</span></div></label>' +
      '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="wrong_item"><div class="safely-report-reason-text"><span class="safely-report-reason-name">Wrong item</span><span class="safely-reason-desc">Received something different</span></div></label>' +
      '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="non_responsive"><div class="safely-report-reason-text"><span class="safely-report-reason-name">Non responsive</span><span class="safely-reason-desc">Seller stopped responding after payment</span></div></label>' +
      "</div>" +
      '<button class="safely-report-btn" id="safely-report-submit">Submit Report</button>' +
      '<div class="safely-report-success" id="safely-report-success" style="display:none">' +
      "<span>&#10003;</span> Report submitted. Thank you for helping protect the community." +
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
      '" id="safely-risk-subtab-seller">Risk</button>' +
      '<button class="safely-sub-tab' +
      (!sellerVisible ? " safely-active" : "") +
      '" id="safely-risk-subtab-report">Report</button>' +
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
          alert("Please select a reason before submitting.");
          return;
        }

        const pageData = (window as any).__safelyData;
        submitBtn.textContent = "Submitting...";
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
          submitBtn.textContent = "Submit Report";
          submitBtn.disabled = false;
          if (result && result.error === "unauthorized") {
            alert("Please sign in again to submit a report.");
          } else {
            alert("Failed to submit report. Please try again.");
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
        // number appears in it.
        if ((window as any).__safelyData.seller.networkSummary) {
          (window as any).__safelyData.seller.networkSummary = (
            window as any
          ).__safelyData.seller.networkSummary.replace(/\d+/, String(newCount));
        }

        const sellerContentEl = document.getElementById("safely-risk-seller-content");
        if (sellerContentEl) sellerContentEl.innerHTML = buildSellerSection();
      });
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

  window.addEventListener("safely-data-ready", () => {
    const tabEl = document.getElementById("safely-tab-risk");
    if (tabEl) {
      tabEl.innerHTML = buildRiskTab();
      attachRiskTabListeners();
    }
  });
})();
