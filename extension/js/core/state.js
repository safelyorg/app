"use strict";
// Tick and cross drawn as icons, so they look the same on every site
// (the packed font has no ✓ / ✕ characters).
const SAFELY_CHECK_ICON = '<svg class="safely-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>';
const SAFELY_CROSS_ICON = '<svg class="safely-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17"/></svg>';
function escapeHtml(str) {
    const div = document.createElement("div");
    div.textContent = str;
    return div.innerHTML;
}
(function () {
    "use strict";
    function defaultData() {
        return {
            analysisId: null,
            riskScore: 0,
            fraudReportCount: 0,
            riskFactors: [],
            seller: {
                name: "Unknown",
                handle: "",
                accountAge: "Unknown",
                verification: "unknown",
                location: "",
                lastActive: "Unknown",
                networkSummary: "Could not connect to Safely server.",
                monthlyActivity: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                platform: "unknown",
                platformId: null,
            },
            signals: [],
        };
    }
    window.__safelyData = defaultData();
    window.__safelyUpdateState = function (newData) {
        window.__safelyData = Object.assign({}, window.__safelyData, newData);
    };
    window.__safelyResetState = function () {
        window.__safelyData = defaultData();
    };
})();
