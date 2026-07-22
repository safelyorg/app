function buildRiskGauge(score, level) {
  var color = riskHex(level);
  var r = 44;
  var circumference = 2 * Math.PI * r;
  var offset = circumference * (1 - score / 100);
  var ticks = "";
  var tickCount = 48;
  for (var i = 0; i < tickCount; i++) {
    var angle = (i * 360) / tickCount;
    var major = i % 6 === 0;
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
    '<svg viewBox="0 0 120 120" class="w-full h-full">' +
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
async function openDetail(analysisId) {
  var panel = document.getElementById("detail-view");
  var loading = document.getElementById("detail-loading");
  var body = document.getElementById("detail-body");
  if (!panel) return;
  panel.classList.remove("hidden");
  loading.classList.remove("hidden");
  loading.textContent = "Loading...";
  body.classList.add("hidden");
  document.getElementById("detail-title").textContent = "";
  try {
    var res = await fetch(API_BASE + "/history/" + analysisId, {
      headers: window.safelyAuth.authHeader(),
    });
    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      loading.textContent = "Could not load this listing.";
      return;
    }
    var data = await res.json();
    renderDetailBody(data);
    loading.classList.add("hidden");
    body.classList.remove("hidden");
  } catch (e) {
    console.error("Safely: failed to load detail", e);
    loading.textContent = "Could not load this listing.";
  }
}
function renderDetailBody(data) {
  var tabBtn = document.querySelector(".detail-tab-btn");
  if (tabBtn && tabBtn.parentElement) {
    tabBtn.parentElement.classList.remove("max-w-md");
    tabBtn.parentElement.classList.add("w-full");
  }
  var intelTab = document.getElementById("detail-tab-content-intel");
  var reportTab = document.getElementById("detail-tab-content-report");
  if (intelTab) intelTab.classList.remove("max-w-2xl");
  if (reportTab) reportTab.classList.remove("max-w-2xl");
  document.getElementById("detail-title").textContent =
    data.listing_title || "Untitled listing";
  var linkEl = document.getElementById("detail-listing-link");
  if (linkEl) {
    if (data.listing_url) {
      linkEl.href = data.listing_url;
      linkEl.classList.remove("hidden");
    } else {
      linkEl.classList.add("hidden");
    }
  }
  document.getElementById("detail-gauge-wrap").innerHTML = buildRiskGauge(
    data.risk_score,
    data.risk_level,
  );
  var levelEl = document.getElementById("detail-risk-level");
  levelEl.textContent =
    data.risk_level.charAt(0).toUpperCase() +
    data.risk_level.slice(1) +
    " risk";
  levelEl.className =
    "text-[15px] font-extrabold mt-1 " + verdictTextClass(data.risk_level);
  document.getElementById("detail-chip-reports").textContent =
    data.fraud_report_count || 0;
  document.getElementById("detail-chip-reports").className =
    "num text-lg font-bold " +
    (data.fraud_report_count > 0 ? "text-coral" : "text-muted");
  var statusText =
    data.seller.verification === "verified"
      ? "Verified"
      : data.seller.verification === "reported"
        ? "Reported"
        : "Unknown";
  document.getElementById("detail-chip-status").textContent = statusText;
  document.getElementById("detail-chip-platform").textContent = data.platform;
  document.getElementById("detail-seller-name").textContent =
    data.seller.name || "Unknown";
  document.getElementById("detail-seller-age").textContent =
    data.seller.account_age || "Unknown";
  document.getElementById("detail-seller-location").textContent =
    data.seller.location || "Unknown";
  document.getElementById("detail-seller-lastactive").textContent =
    data.seller.last_active || "Unknown";
  var chart = document.getElementById("detail-activity-chart");
  var activity = data.seller.monthly_activity || new Array(12).fill(0);
  var max = Math.max.apply(null, activity) || 1;
  chart.innerHTML = activity
    .map(function (v) {
      var heightPx = Math.max(3, Math.round((v / max) * 44));
      return (
        '<div class="flex-1 flex flex-col items-center justify-end relative">' +
        '<span class="text-[9px] text-ink font-bold num absolute top-0">' +
        v +
        "</span>" +
        '<div class="w-full bg-brand/70 rounded-t" style="height:' +
        heightPx +
        'px"></div></div>'
      );
    })
    .join("");
  document.getElementById("detail-network-summary").textContent =
    data.seller.network_summary || "";
  var signals = data.signals || [];
  var badCount = signals.filter(function (s) {
    return s.type !== "good" && s.type !== "info";
  }).length;
  var summaryEl = document.getElementById("detail-intel-summary");
  if (badCount === 0) {
    summaryEl.className =
      "flex gap-2.5 p-3.5 rounded-xl text-[12px] mb-5 bg-mint/10 text-mint";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>All " +
      signals.length +
      " signals checked. No red flags detected.</span>";
  } else {
    summaryEl.className =
      "flex gap-2.5 p-3.5 rounded-xl text-[12px] mb-5 bg-amber/10 text-amber";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>" +
      badCount +
      " of " +
      signals.length +
      " signals need your attention.</span>";
  }
  var signalsList = document.getElementById("detail-signals-list");
  signalsList.innerHTML = signals
    .map(function (s) {
      return (
        '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
        '<div class="flex justify-between items-baseline gap-3">' +
        '<div class="font-semibold text-[13px]">' +
        s.label +
        "</div>" +
        '<div class="text-[13px] font-bold whitespace-nowrap ' +
        signalTextClass(s.type) +
        '">' +
        s.value +
        "</div></div>" +
        '<div class="text-[12px] text-muted mt-1.5">' +
        (s.sub || "") +
        "</div></div>"
      );
    })
    .join("");
  var filedBlock = document.getElementById("detail-report-filed");
  var emptyBlock = document.getElementById("detail-report-empty");
  var reports = data.reports || [];
  if (reports.length > 0) {
    filedBlock.classList.remove("hidden");
    emptyBlock.classList.add("hidden");
    filedBlock.innerHTML = reports
      .map(function (r) {
        return (
          '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
          '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-1.5">Report reason</div>' +
          '<div class="text-[14px] font-semibold">' +
          r.report_type +
          "</div>" +
          '<div class="text-[12px] text-muted mt-2">Submitted ' +
          formatDate(r.reported_at) +
          "</div>" +
          "</div>"
        );
      })
      .join("");
  } else {
    filedBlock.classList.add("hidden");
    filedBlock.innerHTML = "";
    emptyBlock.classList.remove("hidden");
  }
  switchDetailTab("risk");
}
function switchDetailTab(tab) {
  ["risk", "intel", "report"].forEach(function (name) {
    var content = document.getElementById("detail-tab-content-" + name);
    if (content) content.classList.toggle("hidden", name !== tab);
  });
  document.querySelectorAll(".detail-tab-btn").forEach(function (btn) {
    var active = btn.dataset.detailTab === tab;
    btn.classList.toggle("bg-surface3", active);
    btn.classList.toggle("text-ink", active);
  });
}
function closeDetailPanel() {
  var panel = document.getElementById("detail-view");
  if (panel) panel.classList.add("hidden");
}
