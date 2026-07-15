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
    logout: async function () {
      var token = getToken();
      if (token) {
        try {
          await fetch("/api/v1/auth/logout", {
            method: "POST",
            headers: { Authorization: "Bearer " + token },
          });
        } catch (e) {
          console.error("Safely: logout request failed", e);
        }
      }
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

var API_BASE = "/api/v1";
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

function escapeAttr(str) {
  return String(str).replace(/"/g, "&quot;");
}

function detectCardBrand(digits) {
  if (/^4/.test(digits)) return "visa";
  if (/^5[1-5]/.test(digits)) return "mastercard";
  if (/^2(22[1-9]|2[3-9]\d|[3-6]\d{2}|7[01]\d|720)/.test(digits)) {
    return "mastercard";
  }
  return null;
}

function updateCardBrandIcon(digits) {
  var brand = detectCardBrand(digits);
  var visaIcon = document.getElementById("card-brand-visa");
  var mcIcon = document.getElementById("card-brand-mastercard");
  if (visaIcon) visaIcon.classList.toggle("hidden", brand !== "visa");
  if (mcIcon) mcIcon.classList.toggle("hidden", brand !== "mastercard");
}

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

function updateSidebarUserName(name) {
  var el = document.getElementById("sidebar-user-name");
  if (el) el.textContent = name || "User";
}

var currentAvatarObjectUrl = null;

async function updateAvatar(hasAvatar) {
  var targets = [
    { img: "sidebar-avatar-img", placeholder: "sidebar-avatar-placeholder" },
    {
      img: "settings-avatar-img",
      placeholder: "settings-avatar-placeholder",
    },
  ];

  if (!hasAvatar) {
    targets.forEach(function (t) {
      var img = document.getElementById(t.img);
      var placeholder = document.getElementById(t.placeholder);
      if (img) img.classList.add("hidden");
      if (placeholder) placeholder.classList.remove("hidden");
    });
    return;
  }

  try {
    var res = await fetch(API_BASE + "/me/avatar", {
      headers: window.safelyAuth.authHeader(),
    });
    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) throw new Error("Failed to load avatar");

    var blob = await res.blob();

    if (currentAvatarObjectUrl) URL.revokeObjectURL(currentAvatarObjectUrl);
    currentAvatarObjectUrl = URL.createObjectURL(blob);

    targets.forEach(function (t) {
      var img = document.getElementById(t.img);
      var placeholder = document.getElementById(t.placeholder);
      if (img) {
        img.src = currentAvatarObjectUrl;
        img.classList.remove("hidden");
      }
      if (placeholder) placeholder.classList.add("hidden");
    });
  } catch (e) {
    console.error("Safely: failed to load avatar image", e);
  }
}

async function uploadAvatar(file) {
  var errorEl = document.getElementById("settings-avatar-error");
  var label = document.getElementById("settings-avatar-label");
  if (errorEl) errorEl.classList.add("hidden");

  var validTypes = ["image/png", "image/jpeg", "image/webp"];
  if (!validTypes.includes(file.type)) {
    if (errorEl) {
      errorEl.textContent = "Please choose a PNG, JPEG, or WEBP image.";
      errorEl.classList.remove("hidden");
    }
    return;
  }
  if (file.size > 2 * 1024 * 1024) {
    if (errorEl) {
      errorEl.textContent = "Image must be 2MB or smaller.";
      errorEl.classList.remove("hidden");
    }
    return;
  }

  var originalText = label ? label.textContent : "";
  if (label) label.textContent = "Uploading...";

  try {
    var formData = new FormData();
    formData.append("avatar", file);

    var res = await fetch(API_BASE + "/me/avatar", {
      method: "POST",
      headers: window.safelyAuth.authHeader(),
      body: formData,
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      var errBody = await res.text();
      throw new Error(errBody || "Upload failed");
    }

    await res.json();
    updateAvatar(true);
  } catch (e) {
    console.error("Safely: avatar upload failed", e);
    if (errorEl) {
      errorEl.textContent = "Could not upload photo. Please try again.";
      errorEl.classList.remove("hidden");
    }
  } finally {
    if (label) label.textContent = originalText;
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

var settingsLoaded = false;

async function loadSettingsData() {
  var loading = document.getElementById("settings-loading");
  var body = document.getElementById("settings-body");
  if (!loading || !body) return;

  try {
    var res = await fetch(API_BASE + "/me", {
      headers: window.safelyAuth.authHeader(),
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      loading.textContent = "Could not load account settings.";
      return;
    }

    var data = await res.json();

    document.getElementById("settings-email").textContent = data.email || "Unknown";
    document.getElementById("settings-name").textContent = data.name || "User";
    updateSidebarUserName(data.name);
    updateAvatar(data.has_avatar);
    document.getElementById("settings-signin-method").textContent =
      data.signed_in_with === "google" ? "Google" : "Email magic link";
    setGoogleButtonState(data.google_linked);
    document.getElementById("settings-created").textContent = formatDate(data.created_at);
    document.getElementById("settings-last-login").textContent = data.last_login_at
      ? formatDate(data.last_login_at)
      : "Unknown";

    loading.classList.add("hidden");
    body.classList.remove("hidden");
    settingsLoaded = true;
  } catch (e) {
    console.error("Safely: failed to load settings", e);
    loading.textContent = "Could not load account settings.";
  }
}

function toggleProfileEdit(showEdit) {
  var editBtn = document.getElementById("profile-edit-btn");
  var actions = document.getElementById("profile-edit-actions");
  var nameDisplay = document.getElementById("settings-name");
  var nameInput = document.getElementById("settings-name-input");
  var avatarOverlay = document.getElementById("avatar-edit-overlay");
  var errorEl = document.getElementById("settings-name-error");
  if (!editBtn || !actions || !nameDisplay || !nameInput) return;

  if (showEdit) {
    nameInput.value = nameDisplay.textContent === "User" ? "" : nameDisplay.textContent;
    editBtn.classList.add("hidden");
    actions.classList.remove("hidden");
    actions.classList.add("flex");
    nameDisplay.classList.add("hidden");
    nameInput.classList.remove("hidden");
    if (avatarOverlay) {
      avatarOverlay.classList.remove("hidden");
      avatarOverlay.classList.add("flex");
    }
    if (errorEl) errorEl.classList.add("hidden");
    nameInput.focus();
  } else {
    editBtn.classList.remove("hidden");
    actions.classList.add("hidden");
    actions.classList.remove("flex");
    nameDisplay.classList.remove("hidden");
    nameInput.classList.add("hidden");
    if (avatarOverlay) {
      avatarOverlay.classList.add("hidden");
      avatarOverlay.classList.remove("flex");
    }
  }
}

async function saveProfileEdit() {
  var input = document.getElementById("settings-name-input");
  var errorEl = document.getElementById("settings-name-error");
  var saveBtn = document.getElementById("profile-save-btn");
  var newName = input.value.trim();
  var currentName = document.getElementById("settings-name").textContent;

  errorEl.classList.add("hidden");

  if (!newName) {
    errorEl.textContent = "Name cannot be empty.";
    errorEl.classList.remove("hidden");
    return;
  }

  if (newName === currentName) {
    toggleProfileEdit(false);
    return;
  }

  var originalText = saveBtn.textContent;
  saveBtn.disabled = true;
  saveBtn.textContent = "Saving...";

  try {
    var res = await fetch(API_BASE + "/me", {
      method: "PATCH",
      headers: Object.assign(
        { "Content-Type": "application/json" },
        window.safelyAuth.authHeader(),
      ),
      body: JSON.stringify({ name: newName }),
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      var errBody = await res.text();
      throw new Error(errBody || "Request failed");
    }

    document.getElementById("settings-name").textContent = newName;
    updateSidebarUserName(newName);
    toggleProfileEdit(false);
  } catch (e) {
    errorEl.textContent = "Could not save. Please try again.";
    errorEl.classList.remove("hidden");
  } finally {
    saveBtn.disabled = false;
    saveBtn.textContent = originalText;
  }
}

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

function setGoogleButtonState(connected) {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;
  btn.dataset.connected = connected ? "true" : "false";
  btn.disabled = false;
  if (connected) {
    btn.textContent = "Connected";
    btn.classList.remove("hover:bg-surface3");
  } else {
    btn.textContent = "Connect";
    btn.classList.add("hover:bg-surface3");
    btn.classList.remove("border-coral", "text-coral");
  }
}

function wireGoogleButtonHover() {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;
  btn.addEventListener("mouseenter", function () {
    if (btn.dataset.connected === "true") {
      btn.textContent = "Disconnect";
      btn.classList.add("border-coral", "text-coral");
    }
  });
  btn.addEventListener("mouseleave", function () {
    if (btn.dataset.connected === "true") {
      btn.textContent = "Connected";
      btn.classList.remove("border-coral", "text-coral");
    }
  });
}

async function handleGoogleButtonClick() {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;

  if (btn.dataset.connected === "true") {
    btn.disabled = true;
    try {
      var res = await fetch(API_BASE + "/me/google/disconnect", {
        method: "POST",
        headers: window.safelyAuth.authHeader(),
      });
      if (res.status === 401) {
        window.safelyAuth.logout();
        return;
      }
      if (!res.ok) throw new Error("Failed to disconnect");
      setGoogleButtonState(false);

      var signinMethodEl = document.getElementById("settings-signin-method");
      if (signinMethodEl) signinMethodEl.textContent = "Email magic link";

      var statusEl = document.getElementById("google-status-message");
      if (statusEl) {
        statusEl.textContent =
          "Google disconnected. You can sign in using your email magic link.";
        statusEl.classList.remove("hidden");
      }
    } catch (e) {
      alert("Could not disconnect Google. Please try again.");
      btn.disabled = false;
    }
  } else {
    var token = window.safelyAuth.getToken();
    window.location.href =
      API_BASE + "/auth/google/connect?session=" + encodeURIComponent(token);
  }
}

function checkGoogleConnectResult() {
  var params = new URLSearchParams(window.location.search);
  var error = params.get("error");
  var connected = params.get("google_connected");

  if (error === "google_already_linked") {
    alert(
      "That Google account is already connected to a different Safely account.",
    );
  } else if (error === "google_email_mismatch") {
    alert(
      "That Google account uses a different email address than your Safely account. " +
        "Please connect a Google account that uses the same email address.",
    );
  } else if (error === "session_expired") {
    alert("Your session expired - please log in again and retry connecting Google.");
  } else if (connected === "1") {
    // Nothing to alert here - the Settings fetch that already ran (or
    // will run) picks up the new state naturally via google_linked.
  }

  if (error || connected) {
    var url = new URL(window.location.href);
    url.searchParams.delete("error");
    url.searchParams.delete("google_connected");
    history.replaceState(null, "", url.pathname + url.hash);
  }
}

document.addEventListener("DOMContentLoaded", function () {
  checkGoogleConnectResult();

  if (window.safelyAuth && window.safelyAuth.getToken()) {
    loadDashboardData();
  }

  var closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", closeDetailPanel);
  }

  var navHistory = document.getElementById("view-history");
  var navReports = document.getElementById("view-reports");
  if (navHistory) {
    navHistory.addEventListener("change", closeDetailPanel);
    navHistory.addEventListener("click", closeDetailPanel);
  }
  if (navReports) {
    navReports.addEventListener("change", closeDetailPanel);
    navReports.addEventListener("click", closeDetailPanel);
  }

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

  var navSettings = document.getElementById("view-settings");
  if (navSettings) {
    navSettings.addEventListener("change", closeDetailPanel);
    navSettings.addEventListener("click", closeDetailPanel);
    navSettings.addEventListener("change", function () {
      if (!settingsLoaded) loadSettingsData();
    });

    if (navSettings.checked && !settingsLoaded) {
      loadSettingsData();
    }
  }

  window.addEventListener("pageshow", function () {
    var settingsRadio = document.getElementById("view-settings");
    if (settingsRadio && settingsRadio.checked && !settingsLoaded) {
      loadSettingsData();
    }
  });

  var settingsLink = document.getElementById("account-settings-link");
  if (settingsLink) {
    settingsLink.addEventListener("click", function () {
      var menu = document.getElementById("account-menu");
      if (menu) menu.open = false;
    });
  }

  var profileEditBtn = document.getElementById("profile-edit-btn");
  if (profileEditBtn) {
    profileEditBtn.addEventListener("click", function () {
      toggleProfileEdit(true);
    });
  }
  var profileCancelBtn = document.getElementById("profile-cancel-btn");
  if (profileCancelBtn) {
    profileCancelBtn.addEventListener("click", function () {
      toggleProfileEdit(false);
    });
  }
  var profileSaveBtn = document.getElementById("profile-save-btn");
  if (profileSaveBtn) {
    profileSaveBtn.addEventListener("click", saveProfileEdit);
  }
  var nameInput = document.getElementById("settings-name-input");
  if (nameInput) {
    nameInput.addEventListener("keydown", function (e) {
      if (e.key === "Enter") saveProfileEdit();
      if (e.key === "Escape") toggleProfileEdit(false);
    });
  }

  var deleteBtn = document.getElementById("delete-account-btn");
  var deleteConfirmBox = document.getElementById("delete-account-confirm");
  var deleteConfirmEmailEl = document.getElementById("delete-confirm-email");
  var deleteConfirmInput = document.getElementById("delete-confirm-input");
  var deleteConfirmBtn = document.getElementById("delete-confirm-btn");
  var deleteCancelBtn = document.getElementById("delete-cancel-btn");
  var deleteConfirmError = document.getElementById("delete-confirm-error");

  if (deleteBtn && deleteConfirmBox) {
    deleteBtn.addEventListener("click", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent
        : "";
      if (deleteConfirmEmailEl) deleteConfirmEmailEl.textContent = accountEmail;
      if (deleteConfirmInput) deleteConfirmInput.value = "";
      if (deleteConfirmBtn) deleteConfirmBtn.disabled = true;
      if (deleteConfirmError) deleteConfirmError.classList.add("hidden");
      deleteConfirmBox.classList.remove("hidden");
      if (deleteConfirmInput) deleteConfirmInput.focus();
    });
  }

  if (deleteCancelBtn && deleteConfirmBox) {
    deleteCancelBtn.addEventListener("click", function () {
      deleteConfirmBox.classList.add("hidden");
    });
  }

  if (deleteConfirmInput && deleteConfirmBtn) {
    deleteConfirmInput.addEventListener("input", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent.trim().toLowerCase()
        : "";
      var typed = deleteConfirmInput.value.trim().toLowerCase();
      deleteConfirmBtn.disabled = !(typed && typed === accountEmail);
    });
  }

  if (deleteConfirmBtn) {
    deleteConfirmBtn.addEventListener("click", async function () {
      deleteConfirmBtn.disabled = true;
      var originalText = deleteConfirmBtn.textContent;
      deleteConfirmBtn.textContent = "Deleting...";

      try {
        var res = await fetch(API_BASE + "/me", {
          method: "DELETE",
          headers: window.safelyAuth.authHeader(),
        });

        if (res.status === 401) {
          window.safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Failed to delete account");

        window.safelyAuth.clearToken();
        window.location.href = "/";
      } catch (e) {
        if (deleteConfirmError) {
          deleteConfirmError.textContent =
            "Could not delete your account. Please try again.";
          deleteConfirmError.classList.remove("hidden");
        }
        deleteConfirmBtn.disabled = false;
        deleteConfirmBtn.textContent = originalText;
      }
    });
  }
  var googleConnectBtn = document.getElementById("google-connect-btn");
  if (googleConnectBtn) {
    wireGoogleButtonHover();
    googleConnectBtn.addEventListener("click", handleGoogleButtonClick);
  }

  var changePlanBtn = document.getElementById("change-plan-btn");
  var changePlanDropdown = document.getElementById("change-plan-dropdown");

  function togglePlanDropdown(show) {
    if (!changePlanDropdown) return;
    changePlanDropdown.classList.toggle("hidden", !show);
  }

  if (changePlanBtn && changePlanDropdown) {
    changePlanBtn.addEventListener("click", function (e) {
      e.stopPropagation();
      togglePlanDropdown(changePlanDropdown.classList.contains("hidden"));
    });

    document.querySelectorAll(".plan-option").forEach(function (opt) {
      opt.addEventListener("click", function () {
        var planName = opt.dataset.plan;
        var planPrice = opt.dataset.price;

        var nameEl = document.getElementById("current-plan-name");
        var priceEl = document.getElementById("current-plan-price");
        if (nameEl) nameEl.textContent = planName;
        if (priceEl) priceEl.textContent = planPrice + " \u00b7 Renews Aug 9, 2026";

        document.querySelectorAll(".plan-option .plan-check").forEach(function (c) {
          c.classList.add("hidden");
        });
        var check = opt.querySelector(".plan-check");
        if (check) check.classList.remove("hidden");

        togglePlanDropdown(false);
      });
    });

    document.addEventListener("click", function (e) {
      if (
        !changePlanDropdown.classList.contains("hidden") &&
        !changePlanDropdown.contains(e.target) &&
        e.target !== changePlanBtn
      ) {
        togglePlanDropdown(false);
      }
    });
  }
  var addPaymentBtn = document.getElementById("add-payment-btn");
  var paymentForm = document.getElementById("payment-form");
  if (addPaymentBtn && paymentForm) {
    addPaymentBtn.addEventListener("click", function () {
      paymentForm.classList.remove("hidden");
    });
  }
  var paymentCancelBtn = document.getElementById("payment-cancel-btn");
  if (paymentCancelBtn && paymentForm) {
    paymentCancelBtn.addEventListener("click", function () {
      paymentForm.classList.add("hidden");
    });
  }
  var cardNumberInput = document.getElementById("card-number-input");
  if (cardNumberInput) {
    cardNumberInput.addEventListener("input", function (e) {
      var digits = e.target.value.replace(/\D/g, "").slice(0, 16);
      var groups = digits.match(/.{1,4}/g) || [];
      e.target.value = groups.join(" ");
      updateCardBrandIcon(digits);
    });
  }

  var cardExpiryInput = document.getElementById("card-expiry-input");
  if (cardExpiryInput) {
    cardExpiryInput.addEventListener("input", function (e) {
      var raw = e.target.value.replace(/\D/g, "").slice(0, 4);

      if (raw.length === 0) {
        e.target.value = "";
        return;
      }

      if (raw.length === 1) {
        if (parseInt(raw, 10) >= 2) {
          e.target.value = "0" + raw + "/";
        } else {
          e.target.value = raw;
        }
        return;
      }

      var month = parseInt(raw.slice(0, 2), 10);
      if (month === 0) month = 1;
      if (month > 12) month = 12;
      var monthStr = month < 10 ? "0" + month : String(month);

      if (raw.length === 2) {
        e.target.value = monthStr + "/";
      } else {
        e.target.value = monthStr + "/" + raw.slice(2, 4);
      }
    });
  }

  var cardCvcInput = document.getElementById("card-cvc-input");
  if (cardCvcInput) {
    cardCvcInput.addEventListener("input", function (e) {
      e.target.value = e.target.value.replace(/\D/g, "").slice(0, 3);
    });
  }

  var paymentSaveBtn = document.getElementById("payment-save-btn");
  if (paymentSaveBtn) {
    paymentSaveBtn.addEventListener("click", function () {
      var digits = cardNumberInput
        ? cardNumberInput.value.replace(/\D/g, "")
        : "";
      var brand = detectCardBrand(digits);
      var last4 = digits.slice(-4);

      var iconEl = document.getElementById("payment-method-icon");
      var labelEl = document.getElementById("payment-method-label");

      if (last4.length === 4 && iconEl && labelEl) {
        if (brand === "visa") {
          iconEl.className =
            "w-9 h-6 rounded bg-[#1a1f71] flex items-center justify-center text-white text-[8px] font-black flex-shrink-0";
          iconEl.textContent = "VISA";
          labelEl.textContent = "Visa ending in " + last4;
        } else if (brand === "mastercard") {
          iconEl.className =
            "w-9 h-6 rounded bg-surface2 border border-line flex items-center justify-center flex-shrink-0";
          iconEl.innerHTML =
            '<svg width="20" height="12" viewBox="0 0 26 16"><circle cx="9" cy="8" r="7" fill="#EB001B"/><circle cx="17" cy="8" r="7" fill="#F79E1B" fill-opacity="0.9"/></svg>';
          labelEl.textContent = "Mastercard ending in " + last4;
        } else {
          iconEl.className =
            "w-9 h-6 rounded bg-surface2 border border-line flex items-center justify-center text-[9px] font-bold text-muted flex-shrink-0";
          iconEl.textContent = "CARD";
          labelEl.textContent = "Card ending in " + last4;
        }
        labelEl.classList.remove("text-muted");
      }

      if (paymentForm) paymentForm.classList.add("hidden");

      alert(
        "Payment processing isn't wired up yet - this is a design preview. " +
          "The card details above are shown for preview only and were not actually saved anywhere.",
      );
    });
  }

  var termsBtn = document.getElementById("terms-privacy-btn");
  var termsModal = document.getElementById("terms-privacy-modal");
  var termsClose = document.getElementById("terms-privacy-close");
  var termsBackdrop = document.getElementById("terms-privacy-backdrop");

  function toggleTermsModal(show) {
    if (!termsModal) return;
    termsModal.classList.toggle("hidden", !show);
    termsModal.classList.toggle("flex", show);
  }

  if (termsBtn) {
    termsBtn.addEventListener("click", function () {
      toggleTermsModal(true);
    });
  }
  if (termsClose) {
    termsClose.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  if (termsBackdrop) {
    termsBackdrop.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") toggleTermsModal(false);
  });

  var avatarInput = document.getElementById("settings-avatar-input");
  if (avatarInput) {
    avatarInput.addEventListener("change", function (e) {
      var file = e.target.files && e.target.files[0];
      if (file) uploadAvatar(file);
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

  document.addEventListener("click", function (e) {
    var menu = document.getElementById("account-menu");
    if (menu && menu.open && !menu.contains(e.target)) {
      menu.open = false;
    }
  });
});
