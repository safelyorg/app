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
          return '<div style="padding:10px 12px;font-size:12px;color:#8a8a93">No platform data</div>';
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
              '<span style="font-size:11px;color:#8a8a93">' +
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

  var currentRiskSubTab = "seller";

  // Same palette as the dashboard's RISK_HEX map - keeping this in sync
  // between the two surfaces is what makes them read as one product.
  var RISK_HEX = { low: "#35d0a6", caution: "#f2b84c", high: "#ff5d5d" };

  // Segmented tick-mark gauge - the same drawing technique used on the
  // dashboard's detail panel, sized down slightly for the panel's width.
  function buildRiskGauge(score, level) {
    var color = RISK_HEX[level] || RISK_HEX.high;
    var r = 44;
    var circumference = 2 * Math.PI * r;
    var offset = circumference * (1 - score / 100);

    var ticks = "";
    var tickCount = 40;
    for (var i = 0; i < tickCount; i++) {
      var angle = (i * 360) / tickCount;
      var major = i % 5 === 0;
      var len = major ? 6 : 3;
      var outerR = 54;
      var innerR = outerR - len;
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

  function buildSellerSection() {
    var pageData = window.__safelyData;
    var score = pageData.riskScore || 0;
    var lvl = wasm.risk_level(score);
    var riskLabel = wasm.risk_label(lvl);
    var riskDesc = wasm.risk_desc(lvl);
    var riskColor = RISK_HEX[lvl] || RISK_HEX.high;

    var activityBars = wasm.build_activity_bars(
      new Uint8Array(
        pageData.seller.monthlyActivity.map(function (v) {
          return Math.min(255, Math.max(0, v));
        }),
      ),
    );

    // 1. Gauge
    var circleHTML =
      '<div class="safely-risk-hero" style="text-align:center;padding:20px 16px 10px">' +
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

    // 2. Seller Information
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
      (pageData.fraudReportCount > 0 ? "#ff5d5d" : "#8a8a93") +
      '">' +
      (pageData.fraudReportCount || 0) +
      '</span></div><div class="safely-seller-detail"><span>Platform</span><span style="text-transform:capitalize">' +
      (pageData.seller.platform || "Unknown") +
      "</span></div></div>";

    // 3. Monthly visit activity
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
              '<span style="flex:1;text-align:center;font-size:8px;color:#8a8a93;">' +
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

    return circleHTML + sellerCardHTML + activityHTML + networkHTML;
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

    var submitBtn = root.querySelector("#safely-report-submit");
    if (submitBtn) {
      submitBtn.addEventListener("click", async function () {
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

        var authHeaders = {};
        try {
          var result = await chrome.storage.local.get("safely_session_token");
          if (result.safely_session_token) {
            authHeaders = {
              Authorization: "Bearer " + result.safely_session_token,
            };
          }
        } catch (e) {
          // fall back to anonymous if storage is unavailable
        }

        fetch("http://localhost:3000/api/v1/report", {
          method: "POST",
          headers: Object.assign(
            { "Content-Type": "application/json" },
            authHeaders,
          ),
          body: JSON.stringify({
            platform: pageData.seller.platform || "olx",
            platform_id: pageData.seller.platformId || null,
            report_type: selected.value,
            description: null,
            listing_url: window.location.href,
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

            // Reflect the report immediately without another /analyze
            // call - each analyze() adds one to that month's visit count,
            // so re-fetching here just to refresh "reported" status would
            // quietly double-count this visit. Updating the already-loaded
            // data in place and redrawing only the seller section avoids
            // that entirely.
            // Mirrors the exact fraud-count contribution used by the
            // backend's calculate_risk_score (services/scoring.rs) - a step
            // function, not a flat +N per report, so the increment has to
            // match brackets exactly rather than guessing at a fixed bump.
            function fraudCountContribution(count) {
              if (count === 0) return 0;
              if (count === 1) return 20;
              if (count === 2) return 35;
              return 50;
            }

            var oldCount = window.__safelyData.fraudReportCount || 0;
            var newCount = oldCount + 1;
            var delta =
              fraudCountContribution(newCount) -
              fraudCountContribution(oldCount);

            window.__safelyData.fraudReportCount = newCount;
            window.__safelyData.seller.verification = "reported";
            window.__safelyData.riskScore = Math.min(
              100,
              (window.__safelyData.riskScore || 0) + delta,
            );

            // The network-alert sentence ("2 fraud reports found on Safely
            // network...") is plain text that came from the server on the
            // last analyze call - it doesn't recompute itself just because
            // fraudReportCount changed. Swap in the new count directly
            // wherever a standalone number appears in that sentence.
            if (window.__safelyData.seller.networkSummary) {
              window.__safelyData.seller.networkSummary =
                window.__safelyData.seller.networkSummary.replace(
                  /\d+/,
                  String(newCount),
                );
            }

            var sellerContent = document.getElementById(
              "safely-risk-seller-content",
            );
            if (sellerContent) sellerContent.innerHTML = buildSellerSection();
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
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8a8a93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>',
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
