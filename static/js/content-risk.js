(async function () {
  "use strict";

  var wasm;
  try {
    var wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
    wasm = await import(wasmUrl);
    await wasm.default();
  } catch (e) {
    console.warn("Safely: WASM blocked, using JS fallback");
    wasm = {
      risk_level: function (s) {
        return s <= 33 ? "low" : s <= 66 ? "caution" : "high";
      },
      risk_label: function (l) {
        return l === "low"
          ? "Low risk"
          : l === "caution"
            ? "Caution"
            : "High risk";
      },
      risk_desc: function (l) {
        return l === "low"
          ? "Safe to proceed"
          : l === "caution"
            ? "Review before proceeding"
            : "High risk detected";
      },
      build_activity_bars: function (a) {
        return "";
      },
      build_platform_rows: function (j) {
        return "";
      },
      verification_badge: function (s) {
        return '<span class="safely-verified-badge">' + s + "</span>";
      },
      status_icon: function (l) {
        return "";
      },
    };
  }

  if (!window.__safelyAddTab) return;

  function buildRiskTab() {
    var pageData = window.__safelyData;
    var lvl = wasm.risk_level(pageData.riskScore);
    var activityBars = wasm.build_activity_bars(
      new Uint8Array(pageData.seller.monthlyActivity),
    );
    var platformRows = wasm.build_platform_rows(
      JSON.stringify(pageData.seller.platforms),
    );
    return (
      '<div class="safely-risk-row"><div><div class="safely-risk-number safely-risk-' +
      lvl +
      '">' +
      pageData.riskScore +
      '</div><div class="safely-risk-label safely-risk-' +
      lvl +
      '">' +
      wasm.risk_label(lvl) +
      '</div><div class="safely-risk-desc">' +
      wasm.risk_desc(lvl) +
      '</div></div><div class="safely-status-circle safely-circle-' +
      lvl +
      '">' +
      wasm.status_icon(lvl) +
      "</div></div>" +
      '<div class="safely-bar-track"><div class="safely-bar-fill safely-bar-' +
      lvl +
      '" style="width:' +
      pageData.riskScore +
      '%"></div></div>' +
      '<div class="safely-chips-row"><div class="safely-chip"><div class="safely-chip-num">' +
      pageData.seller.totalDeals +
      '</div><div class="safely-chip-lbl">Total Deals</div></div><div class="safely-chip"><div class="safely-chip-num">' +
      pageData.seller.disputes +
      '</div><div class="safely-chip-lbl">Disputes</div></div><div class="safely-chip"><div class="safely-chip-num">' +
      pageData.seller.completionRate +
      '</div><div class="safely-chip-lbl">Completion Rate</div></div></div>' +
      '<div class="safely-section-label">Seller Information</div><div class="safely-seller-card"><div class="safely-seller-name">' +
      pageData.seller.name +
      '</div><div class="safely-seller-handle">' +
      pageData.seller.handle +
      '</div><div class="safely-seller-detail"><span>Account age</span><span>' +
      pageData.seller.accountAge +
      '</span></div><div class="safely-seller-detail"><span>Location</span><span>' +
      pageData.seller.location +
      '</span></div><div class="safely-seller-detail"><span>Last active</span><span>' +
      pageData.seller.lastActive +
      '</span></div><div class="safely-seller-detail"><span>Verification</span>' +
      wasm.verification_badge(pageData.seller.verification) +
      "</div></div>" +
      '<div class="safely-section-label" style="margin-top:18px">Platform presence</div><div class="safely-platform-card">' +
      platformRows +
      "</div>" +
      '<div class="safely-section-label" style="margin-top:18px">Deal activity \u2014 12 months</div><div class="safely-activity-card"><div style="display:flex;align-items:flex-end;gap:3px;height:32px">' +
      activityBars +
      '</div><div style="display:flex;justify-content:space-between;margin-top:5px"><span style="font-size:10px;color:#636366">Jul</span><span style="font-size:10px;color:#636366">Jun</span></div></div>' +
      '<div class="safely-network-alert safely-alert-' +
      lvl +
      '" style="margin-top:14px"><span>&#9679;</span><span>' +
      pageData.seller.networkSummary +
      "</span></div>"
    );
  }

  window.__safelyAddTab(
    "risk",
    "Risk & Seller",
    buildRiskTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
    function () {
      if (window.__safelyPreventInputBubbling)
        window.__safelyPreventInputBubbling();
    },
  );

  window.addEventListener("safely-data-ready", function () {
    var tabEl = document.getElementById("safely-tab-risk");
    if (tabEl) tabEl.innerHTML = buildRiskTab();
  });
})();
