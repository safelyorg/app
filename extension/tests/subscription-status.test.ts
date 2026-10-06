import { describe, it, expect, vi, beforeEach } from "vitest";
import { fakeChrome } from "./setup-chrome";
import "../ts/core/panel-subscription-logic";
import "../ts/core/api";

const formatUsageLine = (globalThis as any).__safelyFormatUsageLine;
const formatResetDate = (globalThis as any).__safelyFormatResetDate;
const scanLimitMessage = (globalThis as any).__safelyScanLimitMessage;

describe("formatUsageLine - the small used/limit line in the panel header", () => {
  it("shows Free scans as used/100", () => {
    expect(formatUsageLine({ plan: "Free", used: 1, limit: 100 })).toBe("1/100");
    expect(formatUsageLine({ plan: "Free", used: 100, limit: 100 })).toBe("100/100");
  });

  it("shows Team scans as used/750, monthly or yearly", () => {
    expect(formatUsageLine({ plan: "Team", used: 2, limit: 750 })).toBe("2/750");
  });

  it("shows Enterprise as used/Unlimited", () => {
    expect(formatUsageLine({ plan: "Enterprise", used: 1, limit: null })).toBe("1/Unlimited");
  });

  it("shows 0 before the first scan", () => {
    expect(formatUsageLine({ plan: "Free", used: 0, limit: 100 })).toBe("0/100");
  });

  it("shows nothing when the usage couldn't be read", () => {
    expect(formatUsageLine(null)).toBe("");
    expect(formatUsageLine({ plan: "", used: 0, limit: 100 })).toBe("");
  });
});

describe("formatResetDate", () => {
  it("turns the server's date into a month and day", () => {
    expect(formatResetDate("2026-11-14")).toBe("November 14");
  });

  it("also reads a full timestamp", () => {
    expect(formatResetDate("2027-01-31T06:10:53Z")).toBe("January 31");
  });

  it("never mentions the 1st when no date is known", () => {
    const text = formatResetDate(null);
    expect(text).not.toContain("1st");
    expect(text).toBe("your next monthly reset");
  });

  it("keeps text it can't read as a date", () => {
    expect(formatResetDate("soon")).toBe("soon");
  });
});

describe("scanLimitMessage - the screen shown when scans are used up", () => {
  it("Free: says 100 free scans and the user's own reset date", () => {
    const message = scanLimitMessage("free_scan_limit_reached", 100, "2026-11-14");
    expect(message).toContain("100 free scans");
    expect(message).toContain("November 14");
    expect(message).toContain("Upgrade to Team or Enterprise");
  });

  it("Free: falls back to 100 when the limit is missing", () => {
    expect(scanLimitMessage("free_scan_limit_reached", null, "2026-11-14")).toContain(
      "100 free scans",
    );
  });

  it("paid: says the plan's limit for this month", () => {
    expect(scanLimitMessage("scan_limit_reached", 750, null)).toBe(
      "You've used all 750 scans included in your plan this month.",
    );
  });

  it("paid: still makes sense without a limit", () => {
    expect(scanLimitMessage("scan_limit_reached", null, null)).toBe(
      "You've used all the scans included in your plan this month.",
    );
  });

  it("never mentions a trial", () => {
    for (const reason of ["free_scan_limit_reached", "scan_limit_reached"]) {
      expect(scanLimitMessage(reason, 100, "2026-11-14").toLowerCase()).not.toContain("trial");
    }
  });
});

describe("api.getScanUsage() feeding into formatUsageLine", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    fakeChrome.storage.local.get = vi.fn().mockResolvedValue({});
    (globalThis as any).fetch = vi.fn();
  });

  it("end-to-end: a Free user with 37 scans used sees 37/100", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        plan_name: null,
        status: null,
        usage: { plan: "Free", interval: null, used: 37, limit: 100, resets_on: "2026-11-14" },
      }),
    });

    const api = (window as any).__safelyAPI;
    expect(formatUsageLine(await api.getScanUsage())).toBe("37/100");
  });

  it("end-to-end: a yearly Enterprise user sees used/Unlimited", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        plan_name: "Enterprise",
        status: "active",
        usage: { plan: "Enterprise", interval: "year", used: 5, limit: null, resets_on: null },
      }),
    });

    const api = (window as any).__safelyAPI;
    expect(formatUsageLine(await api.getScanUsage())).toBe("5/Unlimited");
  });

  it("end-to-end: when the status can't be read, the line stays empty", async () => {
    (globalThis as any).fetch.mockResolvedValue({ ok: false, json: async () => ({}) });

    const api = (window as any).__safelyAPI;
    expect(formatUsageLine(await api.getScanUsage())).toBe("");
  });
});
