(function () {
  "use strict";
  if (!window.__safelyAddTab) return;

  var pageData = window.__safelyData;

  // Risk-specific helpers
  function riskLevel(s) {
    if (s <= 33) return "low";
    if (s <= 66) return "caution";
    return "high";
  }
  function riskLabel(l) {
    if (l === "low") return "Low risk";
    if (l === "caution") return "Caution";
    return "High risk";
  }
  function riskDesc(l) {
    if (l === "low") return "Safe to proceed";
    if (l === "caution") return "Review before proceeding";
    return "High risk detected";
  }
  function statusIcon(l) {
    if (l === "low")
      return '<svg viewBox="0 0 24 24" fill="none" stroke="#1d9bf0" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
    if (l === "caution")
      return '<svg viewBox="0 0 24 24" fill="none" stroke="#ff9f0a" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 9v4"/><path d="M12 17h.01"/><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/></svg>';
    return '<svg viewBox="0 0 24 24" fill="none" stroke="#ff453a" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>';
  }
  function vBadge(s) {
    var d, c, t;
    if (s === "verified") {
      d = "safely-dot-green";
      c = "safely-badge-verified";
      t = "Safely Verified";
    } else if (s === "flagged") {
      d = "safely-dot-red";
      c = "safely-badge-flagged";
      t = "Flagged in Safely Network";
    } else {
      d = "safely-dot-gray";
      c = "safely-badge-unknown";
      t = "Unknown";
    }
    return (
      '<span class="safely-verified-badge ' +
      c +
      '"><span class="safely-badge-dot ' +
      d +
      '"></span>' +
      t +
      "</span>"
    );
  }

  var lvl = riskLevel(pageData.riskScore);
  var activityMax = Math.max.apply(null, pageData.seller.monthlyActivity);
  var activityBars = pageData.seller.monthlyActivity
    .map(function (v) {
      var pct = Math.round((v / activityMax) * 100);
      return (
        '<div class="safely-activity-bar" style="height:' +
        Math.round((pct / 100) * 32) +
        "px;opacity:" +
        (0.3 + (pct / 100) * 0.7).toFixed(2) +
        '"></div>'
      );
    })
    .join("");

  var platformRows = pageData.seller.platforms
    .map(function (p) {
      return (
        '<div class="safely-platform-row"><span class="safely-platform-name">' +
        p.name +
        '</span><span class="safely-platform-status safely-pstatus-' +
        p.type +
        '">' +
        p.status +
        "</span></div>"
      );
    })
    .join("");

  var riskTabHTML =
    '<div class="safely-risk-row"><div><div class="safely-risk-number safely-risk-' +
    lvl +
    '">' +
    pageData.riskScore +
    '</div><div class="safely-risk-label safely-risk-' +
    lvl +
    '">' +
    riskLabel(lvl) +
    '</div><div class="safely-risk-desc">' +
    riskDesc(lvl) +
    '</div></div><div class="safely-status-circle safely-circle-' +
    lvl +
    '">' +
    statusIcon(lvl) +
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
    vBadge(pageData.seller.verification) +
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
    "</span></div>";

  window.__safelyAddTab(
    "risk",
    "Risk & Seller",
    riskTabHTML,
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
    function () {
      if (window.__safelyPreventInputBubbling)
        window.__safelyPreventInputBubbling();
    },
  );
})();
