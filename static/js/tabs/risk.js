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
        var platforms = JSON.parse(j);
        if (!platforms || platforms.length === 0)
          return '<div style="padding:10px 12px;font-size:12px;color:#636366">No platform data</div>';
        return platforms
          .map(function (p) {
            var statusClass =
              p.status && p.status.toLowerCase() === "active"
                ? "active"
                : "none";
            return (
              '<div class="safely-platform-row">' +
              '<span class="safely-platform-name" style="text-transform:capitalize">' +
              p.name +
              "</span>" +
              '<span class="safely-platform-status safely-pstatus-' +
              statusClass +
              '">' +
              p.status +
              "</span>" +
              '<span style="font-size:11px;color:#636366">' +
              p.platform_type +
              "</span>" +
              "</div>"
            );
          })
          .join("");
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

  // track current active sub-tab
  var currentRiskSubTab = "seller";

  function buildSellerSection() {
    var pageData = window.__safelyData;
    var score = pageData.riskScore || 0;
    var lvl = wasm.risk_level(score);
    var riskLabel = wasm.risk_label(lvl);
    var riskDesc = wasm.risk_desc(lvl);
    var riskColor =
      lvl === "low" ? "#34c759" : lvl === "caution" ? "#ff9f0a" : "#ff3b30";

    var activityBars = wasm.build_activity_bars(
      new Uint8Array(
        pageData.seller.monthlyActivity.map(function (v) {
          return Math.min(255, Math.max(0, v));
        }),
      ),
    );
    var platformRows = wasm.build_platform_rows(
      JSON.stringify(pageData.seller.platforms),
    );

    // 1. Circle Risk Score
    var circleHTML =
      '<div class="safely-risk-hero" style="text-align:center;padding:20px 16px 10px">' +
      '<div class="safely-risk-gauge" style="width:120px;height:120px;border-radius:50%;border:6px solid ' +
      riskColor +
      ';display:flex;align-items:center;justify-content:center;margin:0 auto 12px;position:relative">' +
      '<span style="font-size:36px;font-weight:700;color:' +
      riskColor +
      '">' +
      score +
      "</span>" +
      "</div>" +
      '<div style="font-size:18px;font-weight:700;color:#ffffff">' +
      riskLabel +
      "</div>" +
      '<div style="font-size:13px;color:#a0a0a0;margin-top:4px">' +
      riskDesc +
      "</div>" +
      "</div>";

    // 2. Seller Information (Added Fraud Reports and Platform, Status kept, Chips row removed)
    var sellerCardHTML =
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
      (pageData.fraudReportCount > 0 ? "#ff453a" : "#a0a0a0") +
      '">' +
      (pageData.fraudReportCount || 0) +
      '</span></div><div class="safely-seller-detail"><span>Platform</span><span style="text-transform:capitalize">' +
      (pageData.seller.platform || "Unknown") +
      "</span></div></div>";

    // 3. Platform presence
    var platformHTML =
      '<div class="safely-section-label" style="margin-top:18px">Platform presence</div><div class="safely-platform-card">' +
      platformRows +
      "</div>";

    // 4. Monthly visit activity
    var activityHTML =
      '<div class="safely-section-label" style="margin-top:18px">Visit activity \u2014 12 months</div>' +
      '<div class="safely-activity-card">' +
      '<div style="display:flex;align-items:flex-end;gap:3px;height:56px">' +
      activityBars +
      "</div>" +
      '<div style="display:flex;gap:3px;margin-top:4px">' +
      (function () {
        var months = [
          "Jan",
          "Feb",
          "Mar",
          "Apr",
          "May",
          "Jun",
          "Jul",
          "Aug",
          "Sep",
          "Oct",
          "Nov",
          "Dec",
        ];
        return months
          .map(function (m) {
            return (
              '<span style="flex:1;text-align:center;font-size:8px;color:#ffffff;">' +
              m +
              "</span>"
            );
          })
          .join("");
      })() +
      "</div></div>";

    // 5. Network alert
    var networkHTML =
      '<div class="safely-network-alert safely-alert-' +
      lvl +
      '" style="margin-top:14px"><span>&#9679;</span><span>' +
      pageData.seller.networkSummary +
      "</span></div>";

    return (
      circleHTML + sellerCardHTML + platformHTML + activityHTML + networkHTML
    );
  }

  function buildReportSection() {
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

  function buildRiskTab() {
    var sellerVisible = currentRiskSubTab === "seller";
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

  function attachRiskTabListeners() {
    var root = document.getElementById("safely-tab-risk");
    if (!root) return;

    // sub-tab switching
    var sellerBtn = root.querySelector("#safely-risk-subtab-seller");
    var reportBtn = root.querySelector("#safely-risk-subtab-report");
    var sellerContent = root.querySelector("#safely-risk-seller-content");
    var reportContent = root.querySelector("#safely-risk-report-content");

    if (sellerBtn) {
      sellerBtn.addEventListener("click", function () {
        currentRiskSubTab = "seller";
        sellerBtn.classList.add("safely-active");
        reportBtn.classList.remove("safely-active");
        sellerContent.style.display = "";
        reportContent.style.display = "none";
      });
    }

    if (reportBtn) {
      reportBtn.addEventListener("click", function () {
        currentRiskSubTab = "report";
        reportBtn.classList.add("safely-active");
        sellerBtn.classList.remove("safely-active");
        reportContent.style.display = "";
        sellerContent.style.display = "none";
      });
    }

    // report submission
    var submitBtn = root.querySelector("#safely-report-submit");
    if (submitBtn) {
      submitBtn.addEventListener("click", function () {
        var selected = root.querySelector(
          'input[name="safely-report-reason"]:checked',
        );
        if (!selected) {
          alert("Please select a reason before submitting.");
          return;
        }

        var pageData = window.__safelyData;
        submitBtn.textContent = "Submitting...";
        submitBtn.disabled = true;

        fetch("http://localhost:3000/api/v1/report", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            platform: pageData.seller.platform || "olx",
            platform_id: pageData.seller.platformId || null,
            report_type: selected.value,
            description: null,
          }),
        })
          .then(function (res) {
            if (!res.ok) {
              return res.text().then(function (err) {
                throw new Error(err);
              });
            }
            var success = root.querySelector("#safely-report-success");
            if (success) success.style.display = "flex";
            submitBtn.style.display = "none";
          })
          .catch(function (err) {
            submitBtn.textContent = "Submit Report";
            submitBtn.disabled = false;
            alert("Failed to submit: " + err.message);
          });
      });
    }
  }

  window.__safelyAddTab(
    "risk",
    "Risk",
    buildRiskTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
    function () {
      if (window.__safelyPreventInputBubbling)
        window.__safelyPreventInputBubbling();
      attachRiskTabListeners();
    },
  );

  window.addEventListener("safely-data-ready", function () {
    var tabEl = document.getElementById("safely-tab-risk");
    if (tabEl) {
      tabEl.innerHTML = buildRiskTab();
      attachRiskTabListeners();
    }
  });
})();
