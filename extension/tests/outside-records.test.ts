import { describe, it, expect, beforeAll } from "vitest";
import { installSharedGlobals, mountFakePanel, runScript, showResult, signal, supplierData, tab } from "./tab-helpers";

// The "Outside records" section of the Details tab (tabs/intelligence.ts).

beforeAll(async () => {
  installSharedGlobals();
  mountFakePanel(["intelligence"]);
  (window as any).__safelyData = supplierData();
  await runScript("../ts/tabs/intelligence");
});

const details = () => tab("intelligence");
const outside = () => details().querySelector(".safely-outside") as HTMLElement | null;

function withSub(label: string, value: string, type: string, sub: string) {
  return { ...signal(label, value, type), sub };
}

describe("Details tab - outside records", () => {
  it("is left out when there are no outside records", () => {
    showResult(supplierData({ signals: [signal("Phone", "Missing", "caution")] }));
    expect(outside()).toBeNull();
  });

  it("keeps outside records out of the main lists and counts", () => {
    showResult(
      supplierData({
        signals: [
          signal("Phone", "Missing", "caution"),
          signal("Sanctions lists", "Not listed", "good"),
          signal("BBB rating", "F rating", "caution"),
        ],
      }),
    );
    const counts = details().querySelectorAll(".safely-count");
    expect(counts[0].textContent).toBe("1needs a look");
    const main = Array.from(details().querySelectorAll(":scope > .safely-rows, .safely-rows"))
      .filter((el) => !el.closest(".safely-outside"))
      .map((el) => el.textContent)
      .join(" ");
    expect(main).not.toContain("BBB rating");
    expect(outside()?.textContent).toContain("BBB rating");
  });

  it("puts records in groups, warnings first, notes last", () => {
    showResult(
      supplierData({
        signals: [
          signal("Canton Fair", "Canton Fair exhibitor", "good"),
          signal("Sanctions lists", "Not listed", "good"),
          signal("Company number", "Not found", "info"),
          signal("EU VAT number", "VAT number not active", "caution"),
        ],
      }),
    );
    const subs = Array.from(outside()!.querySelectorAll(".safely-subheading")).map((el) => el.textContent);
    expect(subs).toEqual(["Official lists", "Trade shows and chambers"]);
    const firstGroup = outside()!.querySelector(".safely-subheading + .safely-rows")!;
    const titles = Array.from(firstGroup.querySelectorAll(".safely-row-title")).map(
      (el) => el.firstChild?.textContent,
    );
    expect(titles[0]).toBe("EU VAT number");
    expect(titles[titles.length - 1]).toBe("Company number");
    // No count line: each row speaks for itself.
    expect(outside()!.querySelector(".safely-section-summary")).toBeNull();
    // A check that found nothing still shows, as a note at the end.
    const notFound = Array.from(firstGroup.querySelectorAll(".safely-row")).pop()!;
    expect(notFound.querySelector(".safely-mark-ok")?.textContent).toBe("i");
  });

  it("turns a link in the text into an 'Open on' button", () => {
    showResult(
      supplierData({
        signals: [
          withSub(
            "BBB rating",
            "A rating",
            "good",
            "Rated A by the Better Business Bureau. Profile: https://www.bbb.org/us/ny/acme-123",
          ),
        ],
      }),
    );
    const btn = outside()!.querySelector("a.safely-source-btn") as HTMLAnchorElement;
    expect(btn.href).toBe("https://www.bbb.org/us/ny/acme-123");
    expect(btn.textContent).toBe("Open on bbb.org");
    expect(outside()!.textContent).not.toContain("https://");
    expect(outside()!.textContent).not.toContain("Profile");
  });
});
