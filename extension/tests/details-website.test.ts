import { describe, it, expect, beforeAll } from "vitest";
import { installSharedGlobals, mountFakePanel, runScript, showResult, signal, supplierData, tab } from "./tab-helpers";

// The Details tab's "Scam warnings online" section (the whole-web
// Google search and complaint sites), and the "Before you pay" steps of
// a website check: a shop gets shop steps, a factory or supplier
// website (no prices, no way to pay shown) gets supplier steps.

beforeAll(async () => {
  installSharedGlobals();
  mountFakePanel(["intelligence"]);
  (window as any).__safelyData = supplierData();
  await runScript("../ts/tabs/intelligence");
});

const details = () => tab("intelligence");

function section(heading: string): HTMLElement | null {
  const h = Array.from(details().querySelectorAll(".safely-heading")).find((el) => el.textContent === heading);
  return h ? (h.nextElementSibling as HTMLElement) : null;
}

function rowTitles(list: HTMLElement | null): string[] {
  return Array.from(list ? list.querySelectorAll(".safely-row-title") : []).map(
    (el) => (el.firstChild && el.firstChild.textContent) || "",
  );
}

const search = (platform: string, links: Array<[string, string]>) => ({
  platform,
  variant_searched: "Acme",
  found: links.length > 0,
  candidates: links.map(([title, url]) => ({ platform, title, url })),
});

const website = (signals: any[], socialCandidates: any[] = []) =>
  supplierData({ signals, socialCandidates, seller: { platform: "Website", name: "Acme" } });

describe("Scam warnings online", () => {
  it("lists the Google and complaint-site links in their own section, not under Online presence", () => {
    showResult(
      supplierData({
        socialCandidates: [
          search("LinkedIn", [["Acme | LinkedIn", "https://linkedin.com/company/acme"]]),
          search("Google search", [
            ["Acme scam - never shipped", "https://forum.example/acme"],
            ["Acme scam - never shipped", "https://forum.example/acme"],
          ]),
          search("Complaint sites", []),
        ],
      }),
    );
    const warnings = section("Scam warnings online");
    expect(rowTitles(warnings)).toEqual(["Google search"]);
    expect(warnings!.querySelector(".safely-row-value")!.textContent).toBe("1 link");
    expect(warnings!.querySelector("a")!.getAttribute("href")).toBe("https://forum.example/acme");
    expect(rowTitles(section("Online presence"))).toEqual(["LinkedIn"]);
    expect(section("Online presence")!.textContent).not.toContain("Google search");
  });

  it("says so when the searches ran and found nothing", () => {
    showResult(supplierData({ socialCandidates: [search("Google search", []), search("Complaint sites", [])] }));
    expect(section("Scam warnings online")!.textContent).toBe("No scam warnings found for this name");
  });

  it("is not shown when the searches did not run", () => {
    showResult(supplierData({ socialCandidates: [search("Facebook", [])] }));
    expect(section("Scam warnings online")).toBeNull();
  });
});

describe("Before you pay - website checks", () => {
  it("a shop gets the shop steps, not the supplier ones", () => {
    showResult(website([signal("Payment at checkout", "Normal", "good"), signal("Prices", "normal", "good")]));
    const steps = rowTitles(section("Before you pay"));
    expect(steps[0]).toBe("Pay by card or PayPal");
    expect(steps).toContain("Read the return policy first");
    expect(steps).not.toContain("Order a sample first");
  });

  it("a factory or supplier website gets the supplier steps", () => {
    // gloflux.com and xtoinc.com (Nov 3): sell by quote.
    showResult(website([signal("Payment at checkout", "Not shown", "info"), signal("Prices", "Not shown", "info")]));
    const steps = rowTitles(section("Before you pay"));
    expect(steps).toEqual([
      "Pay only to the company's own bank account",
      "Match the business licence to the bank account",
      "Confirm bank details by phone or video",
      "Order a sample first",
      "Ask for a live video call",
    ]);
  });

  it("puts the lookalike and risky-payment steps first", () => {
    showResult(
      website([
        signal("Domain check", "Looks like Amazon", "caution"),
        signal("Payment at checkout", "Risky methods only", "caution"),
      ]),
    );
    const steps = rowTitles(section("Before you pay"));
    expect(steps.slice(0, 3)).toEqual([
      "Buy only on the brand's real website",
      "Don't pay by bank transfer, Pix or crypto",
      "Pay by card or PayPal",
    ]);
  });

  it("a marketplace scan keeps the marketplace steps", () => {
    showResult(supplierData({ signals: [signal("Payment at checkout", "Not shown", "info")] }));
    expect(rowTitles(section("Before you pay"))[0]).toBe("Use buyer protection if the platform offers it");
  });
});
