// ============================================================
// Session handling. Runs first, before anything else.
// ============================================================
(function () {
  "use strict";
  var STORAGE_KEY = "safely_session_token";

  function captureSessionFromUrl() {
    var hash = window.location.hash;
    if (hash.indexOf("#session=") === 0) {
      var token = hash.slice("#session=".length);
      if (token) {
        localStorage.setItem(STORAGE_KEY, token);
      }
      history.replaceState(
        null,
        "",
        window.location.pathname + window.location.search,
      );
    }
  }

  function getToken() {
    return localStorage.getItem(STORAGE_KEY);
  }

  function clearToken() {
    localStorage.removeItem(STORAGE_KEY);
  }

  function renderAuthState() {
    var token = getToken();
    var app = document.getElementById("app");
    var gate = document.getElementById("login-gate");
    if (token) {
      if (app) app.classList.remove("hidden");
      if (gate) gate.classList.add("hidden");
    } else {
      if (app) app.classList.add("hidden");
      if (gate) gate.classList.remove("hidden");
    }
  }

  window.safelyAuth = {
    getToken: getToken,
    clearToken: clearToken,
    authHeader: function () {
      var token = getToken();
      return token ? { Authorization: "Bearer " + token } : {};
    },
    logout: function () {
      clearToken();
      window.location.href = "/";
    },
  };

  captureSessionFromUrl();
  renderAuthState();

  var logoutBtn = document.getElementById("btnLogout");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", function (e) {
      e.preventDefault();
      window.safelyAuth.logout();
    });
  }
})();

// ============================================================
// Colors + helpers - kept in one place so the risk palette (mint/
// amber/coral) stays consistent between rows, chips, gauge, and
// signal list rather than drifting between hand-picked shades.
// ============================================================
var API_BASE = "http://localhost:3000/api/v1";
var currentHistory = [];
var currentReports = [];

var RISK_HEX = { low: "#35d0a6", caution: "#f2b84c", high: "#ff5d5d" };

function riskHex(level) {
  return RISK_HEX[level] || RISK_HEX.high;
}

function verdictTextClass(level) {
  return level === "low"
    ? "text-mint"
    : level === "caution"
      ? "text-amber"
      : "text-coral";
}

function verdictBorderClass(level) {
  return level === "low"
    ? "border-mint"
    : level === "caution"
      ? "border-amber"
      : "border-coral";
}

function signalTextClass(type) {
  return type === "good"
    ? "text-mint"
    : type === "info"
      ? "text-muted"
      : type === "caution"
        ? "text-amber"
        : "text-coral";
}

function formatDate(isoString) {
  return isoString ? isoString.slice(0, 10) : "";
}

// ============================================================
// Real data loading.
// ============================================================
async function loadDashboardData() {
  var headers = window.safelyAuth.authHeader();

  try {
    var historyRes = await fetch(API_BASE + "/history", { headers: headers });
    var reportsRes = await fetch(API_BASE + "/reports", { headers: headers });

    if (historyRes.status === 401 || reportsRes.status === 401) {
      window.safelyAuth.logout();
      return;
    }

    var historyData = await historyRes.json();
    var reportsData = await reportsRes.json();

    currentHistory = historyData.history || [];
    currentReports = reportsData.reports || [];

    renderStats();
    renderHistoryRows();
    renderReportRows();
  } catch (e) {
    console.error("Safely: failed to load dashboard data", e);
  }
}

function renderStats() {
  var checked = document.getElementById("stat-checked-num");
  var reported = document.getElementById("stat-reported-num");
  if (checked) checked.textContent = currentHistory.length;
  if (reported) reported.textContent = currentReports.length;
}

function renderHistoryRows() {
  var tbody = document.getElementById("history-rows");
  if (!tbody) return;

  if (currentHistory.length === 0) {
    tbody.innerHTML =
      '<tr><td colspan="4" class="px-4 py-14 text-center text-muted text-[13px]">No listings analyzed yet. Open a listing on OLX or Facebook with the Safely extension to get started.</td></tr>';
    return;
  }

  tbody.innerHTML = currentHistory
    .map(function (item) {
      var searchText = (
        (item.listing_title || "") +
        " " +
        (item.seller_name || "")
      )
        .toLowerCase()
        .replace(/"/g, "");
      var border = verdictBorderClass(item.risk_level);
      return (
        '<tr data-risk="' +
        item.risk_level +
        '" data-search="' +
        searchText +
        '" data-id="' +
        item.id +
        '" class="history-row border-b border-line last:border-b-0 border-l-[3px] ' +
        border +
        ' hover:bg-surface2/60 cursor-pointer transition">' +
        '<td class="hidden sm:table-cell px-4 py-3.5 text-muted num text-[12px]">' +
        formatDate(item.created_at) +
        "</td>" +
        '<td class="px-4 py-3.5 overflow-hidden"><div class="truncate"><span class="block font-bold truncate">' +
        (item.listing_title || "Untitled listing") +
        '</span><span class="block text-[11px] text-muted truncate">' +
        (item.seller_name || "Unknown seller") +
        "</span></div></td>" +
        '<td class="px-4 py-3.5 num font-extrabold ' +
        verdictTextClass(item.risk_level) +
        '">' +
        item.risk_score +
        "</td>" +
        '<td class="px-4 py-3.5">' +
        (item.reported
          ? '<span class="inline-flex px-2.5 py-0.5 rounded-full text-[10px] font-bold bg-coral/10 text-coral">Reported</span>'
          : '<span class="text-muted text-[12px]">No</span>') +
        "</td>" +
        "</tr>"
      );
    })
    .join("");

  tbody.querySelectorAll(".history-row").forEach(function (tr) {
    tr.addEventListener("click", function () {
      openDetail(tr.dataset.id);
    });
  });
}

function renderReportRows() {
  var tbody = document.querySelector("#panel-reports tbody");
  if (!tbody) return;

  if (currentReports.length === 0) {
    tbody.innerHTML =
      '<tr><td colspan="4" class="px-4 py-14 text-center text-muted text-[13px]">You have not filed any reports yet.</td></tr>';
    return;
  }

  tbody.innerHTML = currentReports
    .map(function (item) {
      return (
        '<tr class="border-b border-line last:border-b-0 border-l-[3px] border-coral">' +
        '<td class="hidden sm:table-cell px-4 py-3.5 text-muted num text-[12px]">' +
        formatDate(item.reported_at) +
        "</td>" +
        '<td class="px-4 py-3.5 font-semibold">' +
        item.report_type +
        "</td>" +
        '<td class="hidden sm:table-cell px-4 py-3.5 text-muted capitalize">' +
        item.platform +
        "</td>" +
        '<td class="px-4 py-3.5 truncate">' +
        (item.seller_name || "Unknown seller") +
        "</td>" +
        "</tr>"
      );
    })
    .join("");
}

// ============================================================
// Signature element: segmented instrument-style risk gauge, same
// arc-drawing technique as the landing page's hero gauge, so the
// dashboard's "risk reading" looks like the same instrument across
// the whole product rather than a plain progress-bar circle.
// ============================================================
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

// ============================================================
// Full rich detail panel - fetches GET /api/v1/history/:id.
// ============================================================
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
  // --- Force full-width tabs and content ---
  var tabBtn = document.querySelector(".detail-tab-btn");
  if (tabBtn && tabBtn.parentElement) {
    tabBtn.parentElement.classList.remove("max-w-md");
    tabBtn.parentElement.classList.add("w-full");
  }
  var intelTab = document.getElementById("detail-tab-content-intel");
  var reportTab = document.getElementById("detail-tab-content-report");
  if (intelTab) intelTab.classList.remove("max-w-2xl");
  if (reportTab) reportTab.classList.remove("max-w-2xl");
  // -----------------------------------------

  document.getElementById("detail-title").textContent =
    data.listing_title || "Untitled listing";

  // --- Risk tab ---
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
      : data.seller.verification === "flagged"
        ? "Flagged"
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

  // --- Intelligence tab ---
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
        '<div class="flex justify-between gap-3 px-4 py-3.5 border-b border-line last:border-b-0 border-l-[3px] ' +
        (s.type === "good"
          ? "border-mint"
          : s.type === "info"
            ? "border-line"
            : s.type === "caution"
              ? "border-amber"
              : "border-coral") +
        '"><div class="min-w-0"><div class="font-semibold text-[13px]">' +
        s.label +
        '</div><div class="text-[11px] text-muted mt-0.5">' +
        (s.sub || "") +
        '</div></div><div class="text-[13px] font-bold whitespace-nowrap ' +
        signalTextClass(s.type) +
        '">' +
        s.value +
        "</div></div>"
      );
    })
    .join("");

  // --- Report tab ---
  var filedBlock = document.getElementById("detail-report-filed");
  var emptyBlock = document.getElementById("detail-report-empty");
  if (data.reported) {
    filedBlock.classList.remove("hidden");
    emptyBlock.classList.add("hidden");
    document.getElementById("detail-report-reason").textContent =
      data.report_reason || "";
    document.getElementById("detail-report-date").textContent = data.report_date
      ? "Submitted " + formatDate(data.report_date)
      : "";
  } else {
    filedBlock.classList.add("hidden");
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

document.addEventListener("DOMContentLoaded", function () {
  if (window.safelyAuth && window.safelyAuth.getToken()) {
    loadDashboardData();
  }

  var closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", closeDetailPanel);
  }

  // Clicking History/Reports in the sidebar switches the underlying panel
  // via CSS just fine on its own - but the detail view sits in a separate,
  // JS-controlled overlay on top of everything, so nothing told it to
  // close when nav was clicked. Without this, a nav click while a detail
  // was open appeared to do nothing, since the detail overlay kept
  // covering the panel that had actually switched underneath it.
  var navHistory = document.getElementById("view-history");
  var navReports = document.getElementById("view-reports");
  if (navHistory) navHistory.addEventListener("change", closeDetailPanel);
  if (navReports) navReports.addEventListener("change", closeDetailPanel);

  // Also close the mobile drawer whenever a nav item is chosen there.
  var mobileToggle = document.getElementById("mobile-nav-toggle");
  if (mobileToggle) {
    [navHistory, navReports].forEach(function (input) {
      if (input) {
        input.addEventListener("change", function () {
          mobileToggle.checked = false;
        });
      }
    });
  }

  document.querySelectorAll(".detail-tab-btn").forEach(function (btn) {
    btn.addEventListener("click", function () {
      switchDetailTab(btn.dataset.detailTab);
    });
  });

  var searchBox = document.getElementById("search-box");
  if (searchBox) {
    searchBox.addEventListener("input", function (e) {
      var q = e.target.value.trim().toLowerCase();
      document.querySelectorAll("#history-rows tr").forEach(function (tr) {
        var haystack = tr.getAttribute("data-search") || "";
        tr.setAttribute(
          "data-search-hidden",
          q && haystack.indexOf(q) === -1 ? "true" : "false",
        );
      });
    });
  }
});
