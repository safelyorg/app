import { describe, it, expect, beforeAll } from "vitest";
import {
  installSharedGlobals,
  mountFakePanel,
  runScript,
  showResult,
  signal,
  supplierData,
  tab,
} from "./tab-helpers";

// These tests load the real tabs/intelligence.ts (the "Details" tab)
// and check what it draws.

beforeAll(async () => {
  installSharedGlobals();
  mountFakePanel(["intelligence"]);
  (window as any).__safelyData = supplierData();
  await runScript("../ts/tabs/intelligence");
});

const details = () => tab("intelligence");
const text = () => details().textContent || "";

function section(heading: string): HTMLElement {
  const h = Array.from(details().querySelectorAll(".safely-heading")).find(
    (el) => el.textContent === heading,
  );
  if (!h) throw new Error("no section " + heading);
  return h.nextElementSibling as HTMLElement;
}

// A B2B scan: these labels only come from the supplier checks.
const B2B = signal("Company profile completeness", "Complete", "good");

describe("Details tab - counts and lists", () => {
  it("counts what needs a look and what looks fine", () => {
    showResult(
      supplierData({
        signals: [
          signal("Phone", "Missing", "caution"),
          signal("Website", "Not found", "bad"),
          signal("Account age", "10 years", "good"),
        ],
      }),
    );
    const counts = details().querySelectorAll(".safely-count");
    expect(counts[0].textContent).toBe("2need a look");
    expect(counts[0].classList.contains("safely-count-hot")).toBe(true);
    expect(counts[1].textContent).toBe("1looks fine");
  });

  it("puts problems under 'Needs a look' with a ! mark", () => {
    showResult(supplierData({ signals: [signal("Phone", "Missing", "caution")] }));
    const rows = section("Needs a look").querySelectorAll(".safely-row");
    expect(rows).toHaveLength(1);
    expect(rows[0].querySelector(".safely-mark-flag")?.textContent).toBe("!");
  });

  it("shows 2 fine checks, hides the rest under 'Show more'", () => {
    showResult(
      supplierData({
        signals: ["A", "B", "C", "D", "E"].map((l) => signal(l, "ok", "good")),
      }),
    );
    const list = section("Looks fine");
    expect(list.querySelectorAll(".safely-row")).toHaveLength(5);
    expect(list.querySelectorAll(".safely-row.safely-extra")).toHaveLength(3);
    expect(list.querySelector("[data-safely-more]")?.textContent).toBe("Show 3 more");
  });

  it("has no 'Show more' with 2 or fewer fine checks", () => {
    showResult(supplierData({ signals: [signal("A", "ok", "good"), signal("B", "ok", "good")] }));
    expect(section("Looks fine").querySelector("[data-safely-more]")).toBeNull();
  });

  it("shows info checks as fine, with an 'i' mark", () => {
    showResult(supplierData({ signals: [signal("Founded", "Not provided", "info")] }));
    const row = section("Looks fine").querySelector(".safely-row") as HTMLElement;
    expect(row.querySelector(".safely-mark-ok")?.textContent).toBe("i");
  });

  it("draws the supplier's checklist with a tick for each detail given and a cross for each missing", () => {
    showResult(
      supplierData({
        signals: [
          signal(
            "Company profile completeness",
            "Partial",
            "caution",
            "Some details are missing.###CHECKLIST###Founded|true;Employees|false",
          ),
        ],
      }),
    );
    const items = details().querySelectorAll("li");
    expect(items).toHaveLength(2);
    expect(items[0].className).toBe("safely-has");
    expect(items[0].querySelector("svg")).not.toBeNull();
    expect(items[1].className).toBe("safely-missing");
    expect(text()).toContain("Some details are missing.");
    expect(text()).not.toContain("###CHECKLIST###");
  });

  it("escapes HTML in check names and reasons", () => {
    showResult(
      supplierData({ signals: [signal("<img src=x onerror=alert(1)>", "v", "good", "<b>x</b>")] }),
    );
    expect(details().querySelector("img")).toBeNull();
    expect(details().querySelector("b:not(.safely-count b)")).toBeNull();
  });
});

describe("Details tab - online presence", () => {
  const found = (platform: string, urls: string[]) => ({
    platform,
    variant_searched: "Palmer Holland",
    found: urls.length > 0,
    candidates: urls.map((url) => ({ platform, title: url, url })),
  });

  it("is hidden when nothing was searched", () => {
    showResult(supplierData());
    expect(text()).not.toContain("Online presence");
  });

  it("shows one row per platform with its link count, in a fixed order", () => {
    showResult(
      supplierData({
        socialCandidates: [
          found("LinkedIn", ["https://linkedin.com/a"]),
          found("Facebook", ["https://facebook.com/a", "https://facebook.com/b"]),
        ],
      }),
    );
    const rows = section("Online presence").querySelectorAll(".safely-row");
    expect(rows[0].textContent).toContain("Facebook");
    expect(rows[0].textContent).toContain("2 links");
    expect(rows[1].textContent).toContain("LinkedIn");
    expect(rows[1].textContent).toContain("1 link");
  });

  it("counts the same link found under two names only once", () => {
    showResult(
      supplierData({
        socialCandidates: [
          found("Facebook", ["https://facebook.com/a"]),
          found("Facebook", ["https://facebook.com/a"]),
        ],
      }),
    );
    expect(details().querySelectorAll('a[href="https://facebook.com/a"]')).toHaveLength(1);
    expect(text()).toContain("1 link");
  });

  it("opens links in a new tab", () => {
    showResult(supplierData({ socialCandidates: [found("Facebook", ["https://facebook.com/a"])] }));
    const link = details().querySelector("a.safely-link-out") as HTMLAnchorElement;
    expect(link.target).toBe("_blank");
    expect(link.rel).toContain("noopener");
  });

  it("shows 3 platforms and hides the rest under 'Show more'", () => {
    showResult(
      supplierData({
        socialCandidates: ["Facebook", "LinkedIn", "TikTok", "Instagram"].map((p) =>
          found(p, ["https://" + p.toLowerCase() + ".com/a"]),
        ),
      }),
    );
    const list = section("Online presence");
    expect(list.querySelectorAll(".safely-row.safely-extra")).toHaveLength(1);
    expect(list.querySelector("[data-safely-more]")?.textContent).toBe("Show 1 more");
  });

  it("lists every platform with nothing found on one line, always shown", () => {
    showResult(
      supplierData({
        socialCandidates: [
          found("Facebook", ["https://facebook.com/a"]),
          found("Reddit", []),
          found("Trustpilot", []),
        ],
      }),
    );
    const list = section("Online presence");
    const none = list.querySelector(".safely-mark-none")?.closest(".safely-row") as HTMLElement;
    expect(none.textContent).toBe("Not found on Reddit, Trustpilot");
    expect(none.classList.contains("safely-extra")).toBe(false);
  });
});

describe("Details tab - Before you pay", () => {
  const steps = () =>
    Array.from(section("Before you pay").querySelectorAll(".safely-row-title")).map(
      (el) => el.textContent,
    );

  it("shows the supplier steps on every scan, even without a supplier-only card", () => {
    showResult(supplierData({ signals: [signal("Phone", "Found", "good")] }));
    expect(steps()[0]).toBe("Use buyer protection if the platform offers it");
    expect(steps()).not.toContain("Check IMEI on delivery");
  });

  it("shows the supplier steps on a B2B scan, 3 first and the rest under 'Show more'", () => {
    showResult(supplierData({ signals: [B2B] }));
    expect(steps()).toHaveLength(5);
    expect(steps()[0]).toBe("Use buyer protection if the platform offers it");
    expect(section("Before you pay").querySelector("[data-safely-more]")?.textContent).toBe(
      "Show 2 more",
    );
  });

  it("puts the payment warning first when the supplier wants full payment up front", () => {
    showResult(
      supplierData({
        signals: [B2B, signal("Advance payment request", "Full prepayment", "caution")],
      }),
    );
    expect(steps()[0]).toBe("Don't pay everything before shipment");
  });

  it("warns about Western Union first for an untraceable payment method", () => {
    showResult(
      supplierData({
        signals: [B2B, signal("Advance payment request", "Untraceable payment", "caution")],
      }),
    );
    expect(steps()[0]).toBe("Do not pay by Western Union, MoneyGram or crypto");
  });

  it("puts the inspection step first for a commodity scam pattern", () => {
    showResult(
      supplierData({
        signals: [
          B2B,
          signal("Product range", "Commodity scam pattern", "caution"),
          signal("Advance payment request", "Full prepayment", "caution"),
        ],
      }),
    );
    expect(steps()[0]).toBe("Don't pay fees before an inspection");
    expect(steps()[1]).toBe("Don't pay everything before shipment");
  });

  it("keeps a ticked step ticked when the tab redraws, and clears it on a new scan", () => {
    showResult(supplierData({ signals: [B2B] }));
    const first = () => section("Before you pay").querySelector(".safely-row") as HTMLElement;

    // panel.ts flips the row first; the tab only remembers it.
    first().classList.add("safely-ticked");
    (first().querySelector("[data-safely-tick]") as HTMLElement).click();

    window.dispatchEvent(new CustomEvent("safely-lang-changed"));
    expect(first().classList.contains("safely-ticked")).toBe(true);
    expect(first().querySelector("[data-safely-tick]")?.getAttribute("aria-checked")).toBe("true");

    showResult(supplierData({ analysisId: "analysis-2", signals: [B2B] }));
    expect(first().classList.contains("safely-ticked")).toBe(false);
  });
});
