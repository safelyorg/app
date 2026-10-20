import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from "vitest";
import {
  installSharedGlobals,
  mountFakePanel,
  runScript,
  showResult,
  signal,
  supplierData,
  tab,
} from "./tab-helpers";

// These tests load the real tabs/risk.ts and check what it draws.

beforeAll(async () => {
  installSharedGlobals();
  mountFakePanel(["risk"]);
  // state.ts sets this before the tabs load.
  (window as any).__safelyData = supplierData();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  await runScript("../ts/tabs/risk");
  await new Promise((resolve) => setTimeout(resolve, 0));
});

beforeEach(() => {
  (window as any).__safelyAPI = {
    submitReport: vi.fn(),
    submitOutcome: vi.fn(),
  };
  (window as any).alert = vi.fn();
  document.getElementById("safely-report-sheet")?.remove();
});

afterEach(() => {
  vi.useRealTimers();
});

const risk = () => tab("risk");
const text = () => risk().textContent || "";
const ringNumber = () => risk().querySelector(".safely-ring-number")?.textContent;

describe("Risk tab - score card", () => {
  it("shows the score and Low risk with no warning signs", () => {
    showResult(
      supplierData({ riskScore: 12, signals: [signal("Account age", "About 11 years", "good")] }),
    );
    expect(ringNumber()).toBe("12");
    expect(risk().querySelector(".safely-level")?.textContent).toBe("Low risk");
    expect(text()).toContain("No warning signs found.");
    expect(text()).toContain("All 1 checks look fine.");
  });

  it("never says 'no warning signs' while a check needs a look", () => {
    showResult(
      supplierData({
        riskScore: 20,
        signals: [signal("Phone", "Missing", "caution"), signal("Account age", "10 years", "good")],
      }),
    );
    expect(text()).not.toContain("No warning signs found.");
    expect(text()).toContain("No serious warning signs.");
    expect(text()).toContain("1 of 2 checks needs a look.");
  });

  it("shows High risk and tells the buyer not to pay yet", () => {
    showResult(supplierData({ riskScore: 80 }));
    expect(risk().querySelector(".safely-level")?.textContent).toBe("High risk");
    expect(text()).toContain("Don't pay this supplier yet.");
  });

  it("says 'Not enough information' when Safely could check too little", () => {
    showResult(
      supplierData({
        riskScore: 34,
        riskFactors: [{ name: "not_enough_information", severity: "soft", description: "x" }],
      }),
    );
    expect(risk().querySelector(".safely-level")?.textContent).toBe("Not enough information");
    expect(text()).toContain("Check this supplier carefully yourself");
  });
});

describe("Risk tab - What we found", () => {
  it("shows at most 3 rows, the most serious problem first and open", () => {
    showResult(
      supplierData({
        riskScore: 70,
        riskFactors: [
          { name: "thin_profile", severity: "soft", description: "Few details.", contributing_signals: [] },
          {
            name: "unsafe_payment",
            severity: "hard",
            description: "Wants Western Union.",
            contributing_signals: ["Advance payment request"],
          },
        ],
        signals: [
          signal("Account age", "10 years", "good", "Old company."),
          signal("Website", "Found", "good", "Has a site."),
        ],
      }),
    );
    const rows = risk().querySelectorAll(".safely-rows .safely-row");
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain("Advance payment request");
    expect(rows[0].textContent).toContain("Serious");
    expect(rows[0].classList.contains("safely-open")).toBe(true);
    expect(rows[1].textContent).toContain("Worth noting");
    expect(rows[2].textContent).toContain("Account age");
    expect(rows[2].classList.contains("safely-open")).toBe(false);
  });

  it("uses the flagged checks when there are no risk factors", () => {
    showResult(
      supplierData({
        signals: [
          signal("Account age", "10 years", "good"),
          signal("Phone", "Missing", "caution", "No phone given."),
        ],
      }),
    );
    const rows = risk().querySelectorAll(".safely-rows .safely-row");
    expect(rows[0].textContent).toContain("Phone");
    expect(rows[0].querySelector(".safely-mark-flag")).not.toBeNull();
    expect(rows[1].querySelector(".safely-mark-ok svg")).not.toBeNull();
  });

  it("does not list 'not enough information' as a finding", () => {
    showResult(
      supplierData({
        riskFactors: [{ name: "not_enough_information", severity: "soft", description: "x" }],
      }),
    );
    expect(text()).not.toContain("What we found");
  });

  it("escapes HTML in the reasons", () => {
    showResult(supplierData({ signals: [signal("<img src=x onerror=alert(1)>", "v", "caution")] }));
    expect(risk().querySelector("img")).toBeNull();
  });
});

describe("Risk tab - supplier card", () => {
  it("shows how long the supplier is on the platform and the report count", () => {
    showResult(supplierData({ fraudReportCount: 2 }));
    expect(text()).toContain("On ThomasNet for");
    expect(text()).toContain("About 11 years");
    expect(text()).toContain("Reports on Safely");
    expect(risk().querySelector(".safely-facts")?.textContent).toContain("2");
  });

  it("hides rows Safely doesn't know", () => {
    showResult(supplierData());
    expect(text()).not.toContain("Phone");
    expect(text()).not.toContain("Last active");
    expect(text()).not.toContain("Status");
  });

  it("hides the visits chart when there were no visits", () => {
    showResult(supplierData({ seller: { monthlyActivity: [0, 0, 0] } }));
    expect(risk().querySelector(".safely-bars")).toBeNull();
    showResult(supplierData({ seller: { monthlyActivity: [1, 0, 3] } }));
    expect(risk().querySelector(".safely-bars")).not.toBeNull();
  });
});

// ---- Reporting -----------------------------------------------------

async function report(reason = "no_delivery"): Promise<void> {
  (risk().querySelector("#safely-report-open") as HTMLButtonElement).click();
  const sheet = document.getElementById("safely-report-sheet") as HTMLElement;
  const option = sheet.querySelector('input[value="' + reason + '"]') as HTMLInputElement;
  option.checked = true;
  option.dispatchEvent(new Event("change"));
  (sheet.querySelector("#safely-report-submit") as HTMLButtonElement).click();
  await vi.advanceTimersByTimeAsync(1700);
}

describe("Risk tab - reporting", () => {
  it("shows the Report link when you haven't reported the supplier", () => {
    showResult(supplierData());
    expect(risk().querySelector("#safely-report-open")).not.toBeNull();
  });

  it("hides the Report link for a supplier you already reported", () => {
    showResult(supplierData({ reportedByYou: true }));
    expect(risk().querySelector("#safely-report-open")).toBeNull();
    expect(text()).toContain("You already reported this supplier.");
  });

  it("opens the report sheet on the panel, with Send disabled until a reason is picked", () => {
    showResult(supplierData());
    (risk().querySelector("#safely-report-open") as HTMLButtonElement).click();
    const sheet = document.getElementById("safely-report-sheet") as HTMLElement;
    expect(sheet.parentElement?.id).toBe("safely-panel");
    expect(sheet.classList.contains("safely-open")).toBe(true);
    expect(sheet.querySelectorAll('input[name="safely-report-reason"]')).toHaveLength(5);
    const send = sheet.querySelector("#safely-report-submit") as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    const option = sheet.querySelector('input[value="scam"]') as HTMLInputElement;
    option.dispatchEvent(new Event("change"));
    expect(send.disabled).toBe(false);
  });

  it("sends the supplier and the reason, then shows the server's new score", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({
      success: true,
      risk_score: 35,
      risk_level: "caution",
    });
    showResult(supplierData({ riskScore: 20, fraudReportCount: 1 }));

    await report("fake_item");

    const sent = (window as any).__safelyAPI.submitReport.mock.calls[0][0];
    expect(sent.platform).toBe("ThomasNet");
    expect(sent.platform_id).toBe("palmer-holland");
    expect(sent.report_type).toBe("fake_item");
    expect(ringNumber()).toBe("35");
    expect(text()).toContain("You reported this supplier.");
    expect(risk().querySelector("#safely-report-open")).toBeNull();
    expect((window as any).__safelyData.fraudReportCount).toBe(2);
    expect((window as any).__safelyData.reportedByYou).toBe(true);
  });

  it("says you already reported it when the server refuses a second report", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({ error: "already_reported" });
    showResult(supplierData({ riskScore: 20, fraudReportCount: 1 }));

    await report();

    expect((window as any).alert).not.toHaveBeenCalled();
    expect(ringNumber()).toBe("20");
    expect((window as any).__safelyData.fraudReportCount).toBe(1);
    expect(text()).toContain("You already reported this supplier.");
    expect(risk().querySelector("#safely-report-open")).toBeNull();
  });

  it("asks you to sign in again when the session ended", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({ error: "unauthorized" });
    showResult(supplierData());

    await report();

    expect((window as any).alert).toHaveBeenCalledWith("Please sign in again to submit a report.");
    const send = document.getElementById("safely-report-submit") as HTMLButtonElement;
    expect(send.disabled).toBe(false);
    expect(send.textContent).toBe("Send report");
  });

  it("works out the supplier score itself when the server sends none", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({ success: true, risk_score: null });
    // 1 report already, so this is the 2nd: at least 35.
    showResult(supplierData({ riskScore: 20, fraudReportCount: 1 }));

    await report();

    expect(ringNumber()).toBe("35");
  });

  it("uses the OLX rule (+20 for the first report) on OLX", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({ success: true, risk_score: null });
    showResult(supplierData({ riskScore: 30, seller: { platform: "OLX" } }));

    await report();

    expect(ringNumber()).toBe("50");
  });

  it("brings the Report link back for the next supplier", async () => {
    vi.useFakeTimers();
    (window as any).__safelyAPI.submitReport.mockResolvedValue({ success: true, risk_score: 25 });
    showResult(supplierData());
    await report();
    expect(risk().querySelector("#safely-report-open")).toBeNull();

    showResult(supplierData({ analysisId: "analysis-2" }));
    expect(risk().querySelector("#safely-report-open")).not.toBeNull();
  });
});

describe("Risk tab - go ahead / walk away", () => {
  it("hides the buttons and says thanks once the answer is saved", async () => {
    (window as any).__safelyAPI.submitOutcome.mockResolvedValue(true);
    showResult(supplierData());
    (risk().querySelector("#safely-outcome-abort") as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect((window as any).__safelyAPI.submitOutcome).toHaveBeenCalledWith("analysis-1", "aborted");
    expect((risk().querySelector("#safely-outcome-buttons") as HTMLElement).style.display).toBe(
      "none",
    );
    expect((risk().querySelector("#safely-outcome-confirmed") as HTMLElement).style.display).toBe(
      "block",
    );
  });

  it("keeps the buttons and says so when saving failed", async () => {
    (window as any).__safelyAPI.submitOutcome.mockResolvedValue(false);
    showResult(supplierData());
    const proceed = risk().querySelector("#safely-outcome-proceed") as HTMLButtonElement;
    proceed.click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(proceed.disabled).toBe(false);
    expect((window as any).alert).toHaveBeenCalledWith(
      "Could not save your response. Please try again.",
    );
  });

  it("makes 'I'll walk away' the strong button on a high-risk supplier", () => {
    showResult(supplierData({ riskScore: 90 }));
    expect(
      risk().querySelector("#safely-outcome-abort")?.classList.contains("safely-btn-dark"),
    ).toBe(true);
    showResult(supplierData({ riskScore: 10 }));
    expect(
      risk().querySelector("#safely-outcome-abort")?.classList.contains("safely-btn-dark"),
    ).toBe(false);
  });
});
