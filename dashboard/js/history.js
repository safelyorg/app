"use strict";
async function loadDashboardData() {
    const headers = window.safelyAuth.authHeader();
    try {
        const meRes = await fetch(API_BASE + "/me", { headers });
        if (meRes.ok) {
            const meData = await meRes.json();
            updateSidebarUserName(meData.name);
            updateAvatar(meData.has_avatar);
        }
    }
    catch (e) {
        console.error("Safely: failed to load account name", e);
    }
}
function renderStats() {
    const checked = document.getElementById("stat-checked-num");
    const high = document.getElementById("stat-high-num");
    const reported = document.getElementById("stat-reported-num");
    if (high) {
        high.textContent = document
            .querySelectorAll('#history-rows tr[data-id][data-risk="high"]')
            .length.toString();
    }
    if (checked) {
        checked.textContent = document
            .querySelectorAll("#history-rows tr[data-id]")
            .length.toString();
    }
    if (reported) {
        // A team list (team_history.ts) counts its own rows marked reported;
        // "My scans" counts your reports.
        const who = window.safelyHistoryWho || "me";
        reported.textContent = (who === "me"
            ? document.querySelectorAll("#report-rows tr[data-report]")
            : document.querySelectorAll('#history-rows tr[data-id][data-reported="true"]')).length.toString();
    }
}
