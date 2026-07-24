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

  try {
    var meRes = await fetch(API_BASE + "/me", { headers: headers });
    if (meRes.ok) {
      var meData = await meRes.json();
      updateSidebarUserName(meData.name);
      updateAvatar(meData.has_avatar);
    }
  } catch (e) {
    console.error("Safely: failed to load account name", e);
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
      '<tr><td colspan="4" class="px-4 py-14 text-center text-muted text-[13px]">No listings analyzed yet. Open a listing on OLX with the Safely extension to get started.</td></tr>';
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
        '" class="history-row border-b border-line last:border-b-0 hover:bg-surface2/60 cursor-pointer transition">' +
        '<td class="hidden sm:table-cell px-4 py-3.5 text-muted num text-[12px]">' +
        formatDate(item.created_at) +
        "</td>" +
        '<td class="px-4 py-3.5 overflow-hidden"><div class="truncate">' +
        '<div class="flex items-center gap-1 min-w-0">' +
        '<span class="font-bold truncate">' +
        (item.listing_title || "Untitled listing") +
        "</span>" +
        (item.listing_url
          ? '<a href="' +
            escapeAttr(item.listing_url) +
            '" target="_blank" rel="noopener noreferrer" class="flex-shrink-0 text-muted hover:text-brand text-[11px]" onclick="event.stopPropagation()" title="Open original listing">\u2197</a>'
          : "") +
        "</div>" +
        '<span class="block text-[11px] text-muted truncate">' +
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
        '<tr class="border-b border-line last:border-b-0">' +
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
        (item.listing_url
          ? '<a href="' +
            escapeAttr(item.listing_url) +
            '" target="_blank" rel="noopener noreferrer" class="hover:underline hover:text-brand">' +
            (item.seller_name || "Unknown seller") +
            " \u2197</a>"
          : item.seller_name || "Unknown seller") +
        "</td>" +
        "</tr>"
      );
    })
    .join("");
}
