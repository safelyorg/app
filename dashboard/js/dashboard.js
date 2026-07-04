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
// Real data loading. Replaces the earlier hardcoded mock rows.
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

function verdictDot(level) {
  return level === "low"
    ? "bg-emerald-400"
    : level === "caution"
      ? "bg-amber-400"
      : "bg-rose-400";
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
    .map(function (item, index) {
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
        '" data-index="' +
        index +
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
      var item = currentHistory[parseInt(tr.dataset.index, 10)];
      openDetail(item, null);
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

  tbody.innerHTML = currentReports
    .map(function (item, index) {
      return (
        '<tr data-index="' +
        index +
        '" class="report-row border-b border-white/5 hover:bg-zinc-800/60 cursor-pointer">' +
        '<td class="px-3 py-3.5 text-zinc-400">' +
        formatDate(item.reported_at) +
        "</td>" +
        '<td class="px-3 py-3.5">' +
        item.report_type +
        "</td>" +
        '<td class="px-3 py-3.5 truncate max-w-[220px]">' +
        item.platform +
        "</td>" +
        '<td class="px-3 py-3.5">' +
        (item.seller_name || "Unknown seller") +
        "</td>" +
        "</tr>"
      );
    })
    .join("");

  tbody.querySelectorAll(".report-row").forEach(function (tr) {
    tr.addEventListener("click", function () {
      var item = currentReports[parseInt(tr.dataset.index, 10)];
      openDetail(null, item);
    });
  });
}

// Single dynamic detail panel - replaces the 5 hardcoded mock detail
// overlays. Shows whatever fields the clicked row actually has; history
// rows and report rows have slightly different fields available.
function openDetail(historyItem, reportItem) {
  var panel = document.getElementById("detail-view");
  if (!panel) return;

  var source = historyItem || reportItem;
  var title = historyItem
    ? historyItem.listing_title || "Untitled listing"
    : "Fraud report";
  var seller = source.seller_name || "Unknown seller";
  var platform = source.platform || "unknown";
  var date = formatDate(
    historyItem ? historyItem.created_at : reportItem.reported_at,
  );

  document.getElementById("detail-title").textContent = title;
  document.getElementById("detail-seller").textContent = seller;
  document.getElementById("detail-platform").textContent = platform;
  document.getElementById("detail-date").textContent = date;

  var riskBlock = document.getElementById("detail-risk-block");
  var reportBlock = document.getElementById("detail-report-block");

  if (historyItem) {
    riskBlock.classList.remove("hidden");
    var scoreEl = document.getElementById("detail-risk-score");
    scoreEl.textContent = historyItem.risk_score;
    scoreEl.className =
      "text-3xl font-extrabold " + verdictColor(historyItem.risk_level);
    document.getElementById("detail-risk-level").textContent =
      historyItem.risk_level.charAt(0).toUpperCase() +
      historyItem.risk_level.slice(1) +
      " risk";
    document.getElementById("detail-reported").textContent =
      historyItem.reported
        ? "You have reported this seller"
        : "You have not reported this seller";
  } else {
    riskBlock.classList.add("hidden");
  }

  if (reportItem) {
    reportBlock.classList.remove("hidden");
    document.getElementById("detail-report-reason").textContent =
      reportItem.report_type;
  } else {
    reportBlock.classList.add("hidden");
  }

  panel.classList.remove("hidden");
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
