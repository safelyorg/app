"use strict";
// Text helpers for the plan / scan-limit messages in the panel. Kept
// separate from panel.ts so they can be tested on their own.
// "2026-11-14" -> "November 14". Falls back to the raw text if the
// date can't be read.
function formatResetDate(isoDate) {
    if (!isoDate)
        return "your next monthly reset";
    const parts = isoDate.slice(0, 10).split("-").map((p) => parseInt(p, 10));
    if (parts.length !== 3 || parts.some((n) => isNaN(n)))
        return isoDate;
    const months = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];
    return months[parts[1] - 1] + " " + parts[2];
}
// The small line under the panel title: scans used / limit, e.g.
// "1/10", "2/750" or "1/Unlimited". Empty string if unknown.
function formatUsageLine(usage) {
    if (!usage || !usage.plan)
        return "";
    return usage.used + "/" + (usage.limit === null ? "Unlimited" : usage.limit);
}
// The message shown when a scan is refused because the limit is used up.
function scanLimitMessage(reason, limit, resetsOn) {
    if (reason === "free_scan_limit_reached") {
        return ("You've used your " +
            (limit || 10) +
            " free scans for this month. Upgrade to Team or Enterprise to keep scanning now, or your free scans come back on " +
            formatResetDate(resetsOn) +
            ".");
    }
    return limit
        ? "You've used all " + limit + " scans included in your plan this month."
        : "You've used all the scans included in your plan this month.";
}
globalThis.__safelyFormatResetDate = formatResetDate;
globalThis.__safelyFormatUsageLine = formatUsageLine;
globalThis.__safelyScanLimitMessage = scanLimitMessage;
