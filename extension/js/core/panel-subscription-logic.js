"use strict";
// Text helpers for the plan / scan-limit messages in the panel. Kept
// separate from panel.ts so they can be tested on their own.
// Translates through i18n.ts when it is loaded; plain English
// otherwise (for example in the tests, which load this file alone).
function subscriptionT(en, vars) {
    const i18n = globalThis.__safelyI18n;
    if (i18n)
        return i18n.t(en, vars);
    if (!vars)
        return en;
    return en.replace(/\{(\w+)\}/g, (whole, key) => Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole);
}
// "2026-11-14" -> "November 14" ("14 de novembro" in Portuguese).
// Falls back to the raw text if the date can't be read.
function formatResetDate(isoDate) {
    if (!isoDate)
        return subscriptionT("your next monthly reset");
    const parts = isoDate.slice(0, 10).split("-").map((p) => parseInt(p, 10));
    if (parts.length !== 3 || parts.some((n) => isNaN(n)))
        return isoDate;
    const i18n = globalThis.__safelyI18n;
    const localized = i18n ? i18n.monthDay(parts[1], parts[2]) : null;
    if (localized)
        return localized;
    const months = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];
    return months[parts[1] - 1] + " " + parts[2];
}
// The small line under the panel title: scans used / limit, e.g.
// "1/5", "2/750" or "1/Unlimited". Empty string if unknown.
function formatUsageLine(usage) {
    if (!usage || !usage.plan)
        return "";
    return usage.used + "/" + (usage.limit === null ? subscriptionT("Unlimited") : usage.limit);
}
// The message shown when a scan is refused because the limit is used up.
function scanLimitMessage(reason, limit, resetsOn) {
    if (reason === "free_scan_limit_reached") {
        return subscriptionT("You've used your {limit} free scans for this month. Upgrade to Team or Enterprise to keep scanning now, or your free scans come back on {date}.", { limit: (limit || 5), date: formatResetDate(resetsOn) });
    }
    return limit
        ? subscriptionT("You've used all {limit} scans included in your plan this month.", { limit })
        : subscriptionT("You've used all the scans included in your plan this month.");
}
globalThis.__safelyFormatResetDate = formatResetDate;
globalThis.__safelyFormatUsageLine = formatUsageLine;
globalThis.__safelyScanLimitMessage = scanLimitMessage;
