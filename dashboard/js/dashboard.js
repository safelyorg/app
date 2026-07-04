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

  // Data loading is triggered from the bottom of this file instead of here,
  // so it only runs after every var/function further down has actually
  // been assigned - calling it this early ran before API_BASE existed yet.
})();

// ============================================================
// Real data loading.
// ============================================================
var API_BASE = "http://localhost:3000/api/v1";
var currentHistory = [];
var currentReports = [];

function verdictColor(level) {
  return level === "low"
    ? "text-emerald-400"
    : level === "caution"
      ? "text-amber-400"
      : "text-rose-400";
}

function verdictBorderColor(level) {
  return level === "low"
    ? "border-emerald-400"
    : level === "caution"
      ? "border-amber-400"
      : "border-rose-400";
}

function verdictDot(level) {
  return level === "low"
    ? "bg-emerald-400"
    : level === "caution"
      ? "bg-amber-400"
      : "bg-rose-400";
}

function signalColor(type) {
  return type === "good"
    ? "text-emerald-400"
    : type === "info"
      ? "text-zinc-400"
      : type === "caution"
        ? "text-amber-400"
        : "text-rose-400";
}

function formatDate(isoString) {
  return isoString ? isoString.slice(0, 10) : "";
}

async function loadDashboardData() {
  var headers = window.safelyAuth.authHeader();

  try {
    var historyRes = await fetch(API_BASE + "/history", { headers: headers });
    var reportsRes = await fetch(API_BASE + "/reports", { headers: headers });

    // A 401 here means the token on file is no longer valid (expired or
    // revoked) - the safe move is to log out cleanly rather than show a
    // broken empty dashboard that looks like a bug.
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
  // The two stat numbers at the top of the History panel are plain
  // <span class="text-xl font-black"> elements with no individual id in
  // the markup - grabbed by position since there are only ever these two.
  var statSpans = document.querySelectorAll(
    "#panel-history .text-xl.font-black",
  );
  if (statSpans[0]) statSpans[0].textContent = currentHistory.length;
  if (statSpans[1]) statSpans[1].textContent = currentReports.length;
}

function renderHistoryRows() {
  var tbody = document.getElementById("history-rows");
  if (!tbody) return;

  if (currentHistory.length === 0) {
    tbody.innerHTML =
      '<tr><td colspan="4" class="px-3 py-8 text-center text-zinc-500">No listings analyzed yet. Open a listing on OLX or Facebook with the Safely extension to get started.</td></tr>';
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
      return (
        '<tr data-risk="' +
        item.risk_level +
        '" data-search="' +
        searchText +
        '" data-id="' +
        item.id +
        '" class="history-row border-b border-white/5 hover:bg-zinc-800/60 cursor-pointer">' +
        '<td class="px-3 py-3.5 text-zinc-400">' +
        formatDate(item.created_at) +
        "</td>" +
        '<td class="px-3 py-3.5"><div class="flex items-center gap-2.5"><span class="w-2 h-2 rounded-full ' +
        verdictDot(item.risk_level) +
        ' flex-shrink-0"></span><span><span class="block font-bold">' +
        (item.listing_title || "Untitled listing") +
        '</span><span class="block text-[11px] text-zinc-500">' +
        (item.seller_name || "Unknown seller") +
        "</span></span></div></td>" +
        '<td class="px-3 py-3.5 font-extrabold ' +
        verdictColor(item.risk_level) +
        '">' +
        item.risk_score +
        "</td>" +
        '<td class="px-3 py-3.5">' +
        (item.reported
          ? '<span class="inline-flex px-2.5 py-0.5 rounded-full text-[11px] font-bold bg-rose-500/10 text-rose-400">Reported</span>'
          : '<span class="text-zinc-500">No</span>') +
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
      '<tr><td colspan="4" class="px-3 py-8 text-center text-zinc-500">You have not filed any reports yet.</td></tr>';
    return;
  }

  // Report rows aren't clickable to a full detail view - fraud_reports
  // isn't tied to a specific analysis id, so there's nothing to fetch
  // detail for yet. Showing them plainly here is honest to what exists.
  tbody.innerHTML = currentReports
    .map(function (item) {
      return (
        '<tr class="border-b border-white/5">' +
        '<td class="px-3 py-3.5 text-zinc-400">' +
        formatDate(item.reported_at) +
        "</td>" +
        '<td class="px-3 py-3.5">' +
        item.report_type +
        "</td>" +
        '<td class="px-3 py-3.5 truncate max-w-[220px] capitalize">' +
        item.platform +
        "</td>" +
        '<td class="px-3 py-3.5">' +
        (item.seller_name || "Unknown seller") +
        "</td>" +
        "</tr>"
      );
    })
    .join("");
}

// ============================================================
// Full rich detail panel - fetches GET /api/v1/history/:id for real
// signals, seller info, and activity data.
// ============================================================
async function openDetail(analysisId) {
  var panel = document.getElementById("detail-view");
  var loading = document.getElementById("detail-loading");
  var body = document.getElementById("detail-body");
  if (!panel) return;

  panel.classList.remove("hidden");
  loading.classList.remove("hidden");
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
  document.getElementById("detail-title").textContent =
    data.listing_title || "Untitled listing";

  // --- Risk tab ---
  var gauge = document.getElementById("detail-risk-gauge");
  gauge.className =
    "w-28 h-28 rounded-full border-4 flex items-center justify-center mx-auto mb-3 " +
    verdictBorderColor(data.risk_level);

  var scoreEl = document.getElementById("detail-risk-score");
  scoreEl.textContent = data.risk_score;
  scoreEl.className =
    "text-3xl font-extrabold " + verdictColor(data.risk_level);

  var levelEl = document.getElementById("detail-risk-level");
  levelEl.textContent =
    data.risk_level.charAt(0).toUpperCase() +
    data.risk_level.slice(1) +
    " risk";
  levelEl.className =
    "text-base font-extrabold " + verdictColor(data.risk_level);

  document.getElementById("detail-chip-reports").textContent =
    data.fraud_report_count || 0;
  document.getElementById("detail-chip-reports").className =
    "text-base font-extrabold " +
    (data.fraud_report_count > 0 ? "text-rose-400" : "text-zinc-500");

  var statusText =
    data.seller.verification === "verified"
      ? "Verified"
      : data.seller.verification === "flagged"
        ? "Flagged"
        : "Unverified";
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
      var heightPx = Math.max(2, Math.round((v / max) * 44));
      return (
        '<div class="flex-1 flex flex-col items-center justify-end relative">' +
        '<span class="text-[8px] text-zinc-500 absolute top-0">' +
        v +
        "</span>" +
        '<div class="w-full bg-sky-400 rounded-t-sm" style="height:' +
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
      "flex gap-2 p-3 rounded-xl text-xs mb-4 bg-emerald-500/10 text-emerald-400";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>All " +
      signals.length +
      " signals checked. No red flags detected.</span>";
  } else {
    summaryEl.className =
      "flex gap-2 p-3 rounded-xl text-xs mb-4 bg-amber-500/10 text-amber-400";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>" +
      badCount +
      " of " +
      signals.length +
      " signals need your attention.</span>";
  }

  var signalsList = document.getElementById("detail-signals-list");
  signalsList.innerHTML = signals
    .map(function (s, i) {
      var borderClass =
        i === signals.length - 1 ? "" : "border-b border-white/5";
      return (
        '<div class="flex justify-between gap-3 px-4 py-3 ' +
        borderClass +
        '"><div><div class="font-semibold text-sm">' +
        s.label +
        '</div><div class="text-[10px] text-zinc-500 mt-0.5">' +
        (s.sub || "") +
        '</div></div><div class="text-sm font-bold whitespace-nowrap ' +
        signalColor(s.type) +
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

  // Reset to the Risk tab every time a new item is opened.
  switchDetailTab("risk");
}

function switchDetailTab(tab) {
  ["risk", "intel", "report"].forEach(function (name) {
    var content = document.getElementById("detail-tab-content-" + name);
    var btn = document.querySelector('[data-detail-tab="' + name + '"]');
    if (content) content.classList.toggle("hidden", name !== tab);
    if (btn) {
      btn.classList.toggle("bg-zinc-700", name === tab);
      btn.classList.toggle("text-white", name === tab);
    }
  });
}

document.addEventListener("DOMContentLoaded", function () {
  // Runs after this entire script has finished its first pass, so
  // API_BASE and every function it depends on are guaranteed to exist.
  if (window.safelyAuth && window.safelyAuth.getToken()) {
    loadDashboardData();
  }

  var closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", function () {
      document.getElementById("detail-view").classList.add("hidden");
    });
  }

  document.querySelectorAll(".detail-tab-btn").forEach(function (btn) {
    btn.addEventListener("click", function () {
      switchDetailTab(btn.dataset.detailTab);
    });
  });

  // ============================================================
  // The one interaction genuinely impossible in pure CSS: substring
  // matching against two fields as the user types.
  // ============================================================
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
