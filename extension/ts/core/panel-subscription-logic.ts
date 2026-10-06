// Text helpers for the plan / scan-limit messages in the panel. Kept
// separate from panel.ts so they can be tested on their own.

// "2026-11-14" -> "November 14". Falls back to the raw text if the
// date can't be read.
function formatResetDate(isoDate: string | null | undefined): string {
  if (!isoDate) return "your next monthly reset";
  const parts = isoDate.slice(0, 10).split("-").map((p) => parseInt(p, 10));
  if (parts.length !== 3 || parts.some((n) => isNaN(n))) return isoDate;
  const months = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];
  return months[parts[1] - 1] + " " + parts[2];
}

// The small line under the panel title: scans used / limit, e.g.
// "1/100", "2/750" or "1/Unlimited". Empty string if unknown.
function formatUsageLine(
  usage: { plan: string; used: number; limit: number | null } | null,
): string {
  if (!usage || !usage.plan) return "";
  return usage.used + "/" + (usage.limit === null ? "Unlimited" : usage.limit);
}

// The message shown when a scan is refused because the limit is used up.
function scanLimitMessage(
  reason: string,
  limit: number | null | undefined,
  resetsOn: string | null | undefined,
): string {
  if (reason === "free_scan_limit_reached") {
    return (
      "You've used your " +
      (limit || 100) +
      " free scans for this month. Upgrade to Team or Enterprise to keep scanning now, or your free scans come back on " +
      formatResetDate(resetsOn) +
      "."
    );
  }
  return limit
    ? "You've used all " + limit + " scans included in your plan this month."
    : "You've used all the scans included in your plan this month.";
}

(globalThis as any).__safelyFormatResetDate = formatResetDate;
(globalThis as any).__safelyFormatUsageLine = formatUsageLine;
(globalThis as any).__safelyScanLimitMessage = scanLimitMessage;
