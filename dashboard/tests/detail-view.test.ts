import { describe, it, expect, beforeAll } from "vitest";
import { loadScript, global } from "./load-script";

// These tests load the real ts/shared.ts and ts/detail_view.ts and
// check the result page they draw (the same layout as the extension).

beforeAll(async () => {
  document.body.innerHTML = '<div id="detail-view"><div id="rows-host"></div></div>';
  await loadScript("ts/shared.ts");
  await loadScript("ts/detail_view.ts");
});

const buildScoreCard = (d: any) => global("buildScoreCard")(d);
const buildFindings = (d: any) => global("buildFindings")(d);
const buildFacts = (d: any) => global("buildFacts")(d);
const buildVisits = (m: any) => global("buildVisits")(m);
const buildDetailsTab = (d: any) => global("buildDetailsTab")(d);
const buildPresence = (r: any) => global("buildPresence")(r);
const buildReports = (r: any) => global("buildReports")(r);

function html(markup: string): HTMLElement {
  const div = document.createElement("div");
  div.innerHTML = markup;
  return div;
}

function signal(label: string, value: string, type: string, sub: string | null = null): any {
  return { label, value, type, sub };
}

function scan(overrides: Record<string, any> = {}): any {
  return {
    listing_title: "Industrial pigments",
    listing_url: "https://www.thomasnet.com/profile/palmer-holland",
    risk_score: 20,
    risk_level: "low",
    fraud_report_count: 0,
    platform: "thomasnet",
    signals: [],
    risk_factors: [],
    reports: [],
    social_candidates: [],
    ...overrides,
    seller: {
      name: "Palmer Holland",
      handle: null,
      phone: "Not found",
      account_age: "About 11 years",
      location: "Ohio, USA",
      last_active: "Unknown",
      verification: "unknown",
      monthly_activity: [],
      network_summary: null,
      ...(overrides.seller || {}),
    },
  };
}

describe("result page - score card", () => {
  it("shows the score and level", () => {
    const card = html(buildScoreCard(scan({ risk_score: 72, risk_level: "high" })));
    expect(card.querySelector(".verdict-number")?.textContent).toBe("72");
    expect(card.querySelector(".verdict-level")?.textContent).toBe("High risk");
    expect(card.textContent).toContain("Don't pay this supplier yet.");
  });

  it("says no warning signs only when every check looks fine", () => {
    const clean = html(
      buildScoreCard(scan({ signals: [signal("Account age", "About 11 years", "good")] })),
    );
    expect(clean.textContent).toContain("No warning signs found.");
    expect(clean.querySelector(".verdict-level")?.classList.contains("is-calm")).toBe(true);

    const flagged = html(buildScoreCard(scan({ signals: [signal("Phone", "Missing", "caution")] })));
    expect(flagged.textContent).toContain("No serious warning signs.");
    expect(flagged.textContent).toContain("1 of 1 checks needs a look.");
  });

  it("says 'Not enough information' when Safely could check too little", () => {
    const card = html(
      buildScoreCard(
        scan({
          risk_score: 34,
          risk_level: "caution",
          risk_factors: [{ name: "not_enough_information", severity: "soft", description: "", contributing_signals: [] }],
        }),
      ),
    );
    expect(card.querySelector(".verdict-level")?.textContent).toBe("Not enough information");
  });

  it("keeps the ring inside 0-100", () => {
    expect(html(buildScoreCard(scan({ risk_score: 140 }))).querySelector(".verdict-number")?.textContent).toBe("100");
  });
});

describe("result page - What we found", () => {
  it("shows up to 3 rows, the most serious problem first and open", () => {
    const list = html(
      buildFindings(
        scan({
          risk_factors: [
            { name: "thin_profile", severity: "soft", description: "Few details.", contributing_signals: [] },
            {
              name: "unsafe_payment",
              severity: "hard",
              description: "Wants Western Union.",
              contributing_signals: ["Advance payment request", "Account age"],
            },
          ],
          signals: [
            signal("Account age", "About 11 years", "good", "Old company."),
            signal("Certificates", "Found", "good", "ISO 9001."),
          ],
        }),
      ),
    );
    const rows = list.querySelectorAll(".d-row");
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain("Advance payment request + Account age");
    expect(rows[0].textContent).toContain("Serious");
    expect(rows[0].classList.contains("is-open")).toBe(true);
    expect(rows[1].textContent).toContain("Worth noting");
    expect(rows[2].classList.contains("is-open")).toBe(false);
  });

  it("is empty when there is nothing to show", () => {
    expect(buildFindings(scan())).toBe("");
  });
});

describe("result page - supplier facts", () => {
  it("shows how long the supplier is on the platform and the report count", () => {
    const facts = html(buildFacts(scan({ fraud_report_count: 3 })));
    expect(facts.textContent).toContain("On ThomasNet for");
    expect(facts.textContent).toContain("About 11 years");
    expect(facts.textContent).toContain("Reports on Safely3");
  });

  it("hides rows Safely doesn't know", () => {
    const facts = html(buildFacts(scan()));
    expect(facts.textContent).not.toContain("Phone");
    expect(facts.textContent).not.toContain("Last active");
    expect(facts.textContent).not.toContain("Status");
  });

  it("shows a status only when the supplier is verified or reported", () => {
    expect(html(buildFacts(scan({ seller: { verification: "reported" } }))).textContent).toContain(
      "StatusReported",
    );
    expect(html(buildFacts(scan({ seller: { verification: "verified" } }))).textContent).toContain(
      "StatusVerified",
    );
  });

  it("hides the visits chart when there were no visits", () => {
    expect(buildVisits([0, 0, 0])).toBe("");
    expect(buildVisits(null)).toBe("");
    expect(html(buildVisits([2, 0, 1])).querySelectorAll(".d-bars i")).toHaveLength(3);
  });
});

describe("result page - Details tab", () => {
  it("counts the checks and hides fine ones after 2 under 'Show more'", () => {
    const tab = html(
      buildDetailsTab(
        scan({
          signals: [
            signal("Phone", "Missing", "caution"),
            ...["A", "B", "C", "D"].map((l) => signal(l, "ok", "good")),
          ],
        }),
      ),
    );
    expect(tab.querySelectorAll(".d-count b")[0].textContent).toBe("1");
    expect(tab.querySelectorAll(".d-count b")[1].textContent).toBe("4");
    expect(tab.querySelectorAll(".d-row.d-extra")).toHaveLength(2);
    expect(tab.querySelector("[data-more]")?.textContent).toBe("Show 2 more");
  });

  it("draws a tick for each detail given and a cross for each missing", () => {
    const tab = html(
      buildDetailsTab(
        scan({
          signals: [
            signal(
              "Company profile completeness",
              "Partial",
              "caution",
              "Some details are missing.###CHECKLIST###Employee count|true;Sales revenue|false",
            ),
          ],
        }),
      ),
    );
    const items = tab.querySelectorAll("li");
    expect(items).toHaveLength(2);
    expect(items[0].classList.contains("is-missing")).toBe(false);
    expect(items[1].classList.contains("is-missing")).toBe(true);
    expect(tab.textContent).not.toContain("###CHECKLIST###");
  });
});

describe("result page - online presence", () => {
  const found = (platform: string, urls: string[]) => ({
    platform,
    variant_searched: "Palmer Holland",
    found: urls.length > 0,
    candidates: urls.map((url) => ({ platform, title: url, url })),
  });

  it("is hidden when nothing was searched", () => {
    expect(buildPresence([])).toBe("");
  });

  it("shows 3 platforms, the rest under 'Show more', and the not-found line", () => {
    const list = html(
      buildPresence([
        found("Instagram", ["https://instagram.com/a"]),
        found("Facebook", ["https://facebook.com/a", "https://facebook.com/b"]),
        found("TikTok", ["https://tiktok.com/a"]),
        found("LinkedIn", ["https://linkedin.com/a"]),
        found("Reddit", []),
      ]),
    );
    const rows = list.querySelectorAll(".d-row");
    expect(rows[0].textContent).toContain("Facebook");
    expect(rows[0].textContent).toContain("2 links");
    expect(list.querySelectorAll(".d-row.d-extra")).toHaveLength(1);
    expect(list.querySelector("[data-more]")?.textContent).toBe("Show 1 more");
    expect(list.textContent).toContain("Not found on Reddit");
  });

  it("counts the same link found twice only once", () => {
    const list = html(
      buildPresence([found("Facebook", ["https://facebook.com/a"]), found("Facebook", ["https://facebook.com/a"])]),
    );
    expect(list.querySelectorAll('a[href="https://facebook.com/a"]')).toHaveLength(1);
  });
});

describe("result page - your reports", () => {
  it("lists each report with its reason and date", () => {
    const list = html(buildReports([{ report_type: "no_delivery", reported_at: "2026-10-20T08:00:00Z" }]));
    expect(list.textContent).toContain("Took payment and never shipped");
    expect(list.textContent).toContain("Submitted 2026-10-20");
  });
});

describe("result page - rows that open and Show more", () => {
  it("opens a row and shows all rows, then back", () => {
    const host = document.getElementById("rows-host") as HTMLElement;
    host.innerHTML = buildDetailsTab(scan({ signals: ["A", "B", "C"].map((l) => signal(l, "ok", "good", "why")) }));

    const opener = host.querySelector("[data-open]") as HTMLElement;
    opener.click();
    expect(opener.closest(".d-row")?.classList.contains("is-open")).toBe(true);
    expect(opener.getAttribute("aria-expanded")).toBe("true");

    const more = host.querySelector("[data-more]") as HTMLElement;
    more.click();
    expect(more.closest(".d-rows")?.classList.contains("show-all")).toBe(true);
    expect(more.textContent).toBe("Show less");
    more.click();
    expect(more.textContent).toBe("Show 1 more");
  });
});
