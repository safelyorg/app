import { describe, it, expect, vi, beforeAll } from "vitest";
import {
  installSharedGlobals,
  mountFakePanel,
  runScript,
  showResult,
  signal,
  supplierData,
  tab,
} from "./tab-helpers";

// The Free plan (backend services/features/results.rs): the Risk tab
// shows the score and the top reasons with a "See the full report"
// card, and the Details tab is locked. Paid plans see both in full.

beforeAll(async () => {
  installSharedGlobals();
  mountFakePanel(["risk", "intelligence"]);
  (window as any).__safelyData = supplierData();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  await runScript("../ts/tabs/risk");
  await runScript("../ts/tabs/intelligence");
  await new Promise((resolve) => setTimeout(resolve, 0));
});

function result(fullResult: boolean) {
  return supplierData({
    riskScore: 72,
    fullResult,
    signals: [
      signal("Price analysis", "Suspiciously low", "bad"),
      signal("Account age", fullResult ? "About 23 years" : "", "good"),
      signal("Sanctions lists", fullResult ? "Not on the lists" : "", "good"),
    ],
  } as any);
}

describe("Free plan", () => {
  it("shows the score, the top reasons and a See the full report card", () => {
    showResult(result(false));
    const risk = tab("risk");
    expect(risk.querySelector(".safely-ring-number")?.textContent).toBe("72");
    expect(risk.textContent).toContain("Price analysis");
    expect(risk.textContent).toContain("1 of 3 checks needs a look.");
    expect(risk.querySelector(".safely-unlock")?.textContent).toContain("See the full report");
    expect(risk.querySelector("[data-safely-unlock]")).not.toBeNull();
  });

  it("locks the Details tab", () => {
    showResult(result(false));
    const details = tab("intelligence");
    expect(details.querySelector(".safely-unlock")).not.toBeNull();
    expect(details.textContent).toContain("See all 3 checks");
    expect(details.textContent).not.toContain("Outside records");
    expect(details.textContent).not.toContain("Sanctions lists");
  });

  it("See plans opens the plans once", () => {
    const open = vi.fn();
    (window as any).open = open;
    (window as any).__safelyAPI = { SITE_BASE: "https://safely.sh" };
    showResult(result(false));
    (tab("intelligence").querySelector("[data-safely-unlock]") as HTMLElement).click();
    expect(open).toHaveBeenCalledTimes(1);
    expect(open.mock.calls[0][0]).toBe("https://safely.sh/dashboard/?manage_billing=1");
  });
});

describe("Paid plans", () => {
  it("show everything, without the lock", () => {
    showResult(result(true));
    expect(tab("risk").querySelector(".safely-unlock")).toBeNull();
    expect(tab("intelligence").querySelector(".safely-unlock")).toBeNull();
    expect(tab("intelligence").textContent).toContain("Sanctions lists");
  });
});
