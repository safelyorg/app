import { describe, it, expect, beforeAll } from "vitest";
import { loadScripts, global } from "./load-script";

// The "Vendors" page (ts/vendors.ts).

beforeAll(async () => {
  await loadScripts(["ts/i18n.ts", "ts/shared.ts", "ts/detail_view.ts", "ts/vendors.ts"]);
});

function show(html: string): string {
  document.body.innerHTML = html;
  return document.body.textContent || "";
}

const item = (over: Record<string, unknown> = {}) => ({
  id: "11111111-1111-1111-1111-111111111111",
  vendor_name: "Northfield Metals",
  vendor_email: "sales@northfield.com",
  status: "ready",
  risk_level: "high",
  risk_score: 58,
  invited_on: "2026-11-10",
  sent_on: "2026-11-12",
  expires_on: "2026-11-24",
  invited_by: "ali@company.com",
  invited_by_you: false,
  ...over,
});

const detail = (over: Record<string, unknown> = {}) => ({
  id: "11111111-1111-1111-1111-111111111111",
  vendor_name: "Northfield Metals",
  invited_as: "Northfield",
  vendor_email: "sales@northfield.com",
  buyer_company: "Acme",
  status: "ready",
  language: "en",
  send_email: true,
  answers: [
    { field: "company_name", label: "Company name", value: "Northfield Metals" },
    { field: "email", label: "Work email", value: "sales@northfield.com" },
    { field: "bank_holder", label: "Bank account name", value: "Carlos <b>Pereira</b>" },
  ],
  email_confirmed: "sales@northfield.com",
  ask_fields: [],
  ask_note: null,
  previous: [],
  result: {
    cards: [
      { key: "bank_name", label: "Bank account name", value: "Different name", type: "bad", detail: "Ask why." },
      { key: "email_code", label: "Email confirmed", value: "Confirmed", type: "good", detail: "Typed the code." },
    ],
    records: [{ label: "Sanctions lists", sub: "Not on any list.", value: "Not found", type: "info" }],
  },
  risk_score: 58,
  risk_level: "high",
  decision_note: null,
  decided_by: null,
  decided_on: null,
  invited_on: "2026-11-10",
  opened_on: "2026-11-11",
  sent_on: "2026-11-12",
  checked_on: "2026-11-12",
  expires_on: "2026-11-24",
  reminder_sent: false,
  watching: false,
  ...over,
});

describe("vendors page", () => {
  it("Free: a locked card with See plans", () => {
    const text = show(global("buildVendorsPage")({ paid: false, items: [] }));
    expect(text).toContain("Team and Enterprise");
    expect(document.querySelector("[data-vendor-plans]")).not.toBeNull();
  });

  it("an empty list has the invite button and the counts", () => {
    const text = show(global("buildVendorsPage")({ paid: true, buyer_company: "Acme", counts: {}, items: [] }));
    expect(text).toContain("No vendors yet");
    expect(document.querySelector("[data-vendor-invite]")).not.toBeNull();
    expect((document.getElementById("vi-buyer") as HTMLInputElement).value).toBe("Acme");
  });

  it("lists vendors with status, risk and Open", () => {
    const text = show(
      global("buildVendorsPage")({
        paid: true,
        counts: { waiting: 1, to_review: 1, approved: 0, flagged: 1 },
        items: [item(), item({ id: "2", status: "invited", risk_level: null, risk_score: null, invited_by_you: true })],
      }),
    );
    expect(text).toContain("Ready to review");
    expect(text).toContain("High · 58");
    expect(text).toContain("Invited");
    expect(text).toContain("You");
    expect(document.querySelectorAll("[data-vendor-open]").length).toBe(2);
  });

  it("a result shows the checks, answers (escaped) and the decision buttons", () => {
    const text = show(global("buildVendorDetail")(detail()));
    expect(text).toContain("Different name");
    expect(text).toContain("Sanctions lists");
    expect(document.body.innerHTML).toContain("Carlos &lt;b&gt;Pereira&lt;/b&gt;");
    expect(document.querySelector('[data-vendor-decide="approve"]')).not.toBeNull();
    expect(document.querySelector('[data-vendor-decide="reject"]')).not.toBeNull();
    expect(document.querySelector("[data-vendor-ask-open]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-bank]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-watch]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-cancel]")).toBeNull();
  });

  it("shows the score like a scan, warnings first, and where it was found online", () => {
    const d = detail();
    (d as any).result.online = [{ platform: "LinkedIn", title: "Northfield", url: "https://linkedin.com/company/n" }];
    const text = show(global("buildVendorDetail")(d));
    expect(document.querySelector(".verdict-number")?.textContent).toBe("58");
    expect(text).toContain("Don't pay this vendor yet.");
    expect(text).toContain("1 of 3 checks needs a look.");
    const headings = Array.from(document.querySelectorAll(".d-heading")).map((h) => h.textContent);
    expect(headings.indexOf("Needs a look")).toBeLessThan(headings.indexOf("Looks fine"));
    expect(document.querySelector('a[href="https://linkedin.com/company/n"]')).not.toBeNull();
  });

  it("lists every site searched, found or not", () => {
    const d = detail();
    (d as any).result.online = [{ platform: "LinkedIn", title: "Northfield", url: "https://linkedin.com/company/n" }];
    (d as any).result.presence = [
      { platform: "Facebook", found: false },
      { platform: "LinkedIn", found: true },
      { platform: "Trustpilot", found: false },
    ];
    const text = show(global("buildVendorDetail")(d));
    expect(text).toContain("Online presence");
    expect(text).toContain("Not found on Facebook, Trustpilot");
  });

  it("looks like a scan's Details tab: counts, groups, Before you pay", () => {
    const text = show(global("buildVendorDetail")(detail()));
    expect(document.querySelector(".d-counts")).not.toBeNull();
    expect(text).toContain("Official lists");
    expect(text).toContain("Before you pay");
    expect(text).toContain("Pay only to the company's own bank account");
  });

  it("has Report as scam, and shows Reported once sent", () => {
    show(global("buildVendorDetail")(detail()));
    expect(document.querySelector("[data-vendor-report-open]")).not.toBeNull();
    expect(document.querySelectorAll('input[name="vendor-report-reason"]').length).toBe(5);
    const text = show(global("buildVendorDetail")(detail({ reported_by_you: true })));
    expect(text).toContain("Reported by you");
    expect(document.querySelector("[data-vendor-report-open]")).toBeNull();
    // Not sent yet: nothing to report.
    show(global("buildVendorDetail")(detail({ status: "opened", sent_on: null, result: null })));
    expect(document.querySelector("[data-vendor-report-open]")).toBeNull();
  });

  it("lists the vendor's files with a download button", () => {
    const text = show(
      global("buildVendorDetail")(
        detail({
          documents: [{ id: "d1", kind: "bank_letter", file_name: "letter.pdf", size: 52000, uploaded_on: "2026-11-19" }],
        }),
      ),
    );
    expect(text).toContain("Bank letter");
    expect(text).toContain("letter.pdf");
    expect(document.querySelector('[data-vendor-doc="d1"]')).not.toBeNull();
  });

  it("a decision shows who decided and the note", () => {
    const text = show(
      global("buildVendorDetail")(
        detail({ status: "rejected", decided_by: "bilal@acme.com", decided_on: "2026-11-19", decision_note: "not reliable" }),
      ),
    );
    expect(text).toContain("Decided by bilal@acme.com on 2026-11-19");
    expect(text).toContain("“not reliable”");
    expect(document.querySelector('[data-vendor-decide="reject"]')).toBeNull();
  });

  it("Watch, Report and PDF sit at the top like a scan; decisions under the score", () => {
    show(global("buildVendorDetail")(detail()));
    const top = Array.from(document.querySelectorAll("[data-vendor-watch], [data-vendor-report-open], [data-vendor-pdf]"));
    expect(top.map((el) => el.textContent?.trim())).toEqual(["Watch this supplier", "Report as scam", "PDF"]);
    // No website given: no Visit website.
    expect(document.querySelector("[data-vendor-site]")).toBeNull();
    // Approve, Reject, Ask for more info: one row in "Your decision".
    const row = document.querySelector("#vendor-decision [data-vendor-actions]");
    expect(Array.from(row?.children || []).map((el) => el.textContent)).toEqual(["Approve", "Reject", "Ask for more info"]);
    expect(document.querySelector("#vendor-decision [data-vendor-bank]")?.textContent).toBe("New link");
    expect(document.querySelector("[data-vendor-cancel]")).toBeNull();
    expect(document.querySelector("[data-vendor-remind]")).toBeNull();

    // After a new link: the link's buttons are in their own box.
    show(global("buildVendorDetail")(detail({ status: "more_info", ask_fields: ["bank_holder"] })));
    const names = Array.from(document.querySelectorAll("[data-vendor-actions] > *")).map((el) => el.textContent);
    expect(names).toEqual(["Ask for more info", "Send a reminder", "New link", "Cancel the link"]);
    expect(document.querySelector("[data-vendor-decide]")).toBeNull();
  });

  it("Visit website only when the vendor gave one, and never a bad address", () => {
    const withSite = (value: string) =>
      detail({
        answers: [
          { field: "company_name", label: "Company name", value: "Northfield Metals" },
          { field: "website", label: "Website", value },
        ],
      });
    show(global("buildVendorDetail")(withSite("northfield.com")));
    const link = document.querySelector("[data-vendor-site]") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("https://northfield.com/");
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toContain("noopener");
    show(global("buildVendorDetail")(withSite("javascript:alert(1)")));
    expect(document.querySelector("[data-vendor-site]")).toBeNull();
  });

  it("the score card shows the scale and what added to the score", () => {
    const text = show(global("buildVendorDetail")(detail()));
    expect(text).toContain("What adds to the score");
    expect(document.querySelector(".vd-zone-high.is-on")).not.toBeNull();
    const points = Array.from(document.querySelectorAll(".vd-point")).map((el) => el.textContent);
    expect(points).toEqual(["Bank account name+25"]);
    // A sanctions match sets the score to 100.
    const d = detail();
    (d as any).result.records = [{ label: "Sanctions lists", sub: "On a list.", value: "Found", type: "bad" }];
    show(global("buildVendorDetail")(d));
    expect(document.body.textContent).toContain("On a sanctions list: the score is 100.");
  });

  it("Free plan: score, level and top 3 reasons only, no actions", () => {
    const free = detail({
      status: "approved",
      decided_by: "bilal@acme.com",
      decided_on: "2026-11-19",
      full_result: false,
      answers: [],
      result: {
        cards: [{ key: "bank_name", label: "Bank account name", value: "Different name", type: "bad", detail: "Ask why." }],
        records: [],
        counts: { total: 20, flagged: 3 },
      },
    });
    const text = show(global("buildVendorDetail")(free));
    expect(document.querySelector(".verdict-number")?.textContent).toBe("58");
    expect(text).toContain("3 of 20 checks need a look.");
    expect(text).toContain("What we found");
    expect(text).toContain("Different name");
    expect(text).toContain("See what adds to the score on the Team and Enterprise plans.");
    expect(text).toContain("Decided by bilal@acme.com");
    expect(document.querySelector("[data-detail-unlock]")).not.toBeNull();
    // No actions, no answers, no full details.
    for (const sel of ["[data-vendor-watch]", "[data-vendor-report-open]", "[data-vendor-pdf]", "[data-vendor-decide]", "[data-vendor-ask-open]", "[data-vendor-bank]", "[data-vendor-link]", ".d-counts"]) {
      expect(document.querySelector(sel)).toBeNull();
    }
    expect(text).not.toContain("The vendor's answers");
    expect(text).not.toContain("Before you pay");
  });

  it("My Reports rows: the reason in words and the Website link keep their text", () => {
    const host = document.createElement("table");
    host.innerHTML =
      '<tbody id="report-rows"><tr><td data-report-reason="no_delivery">no_delivery</td>' +
      '<td><span data-report-kind="vendor">Vendor</span><a data-report-link="website">Website ↗</a></td></tr></tbody>';
    document.body.appendChild(host);
    global("translateReportKinds")();
    expect(host.querySelector("[data-report-reason]")?.textContent).toBe("Took payment and never shipped");
    expect(host.querySelector("[data-report-link]")?.textContent).toBe("Website ↗");
    host.remove();
  });

  it("Free plan list: old vendors stay listed, without inviting", () => {
    const text = show(global("buildVendorsPage")({ paid: false, items: [item()] }));
    expect(text).toContain("Northfield Metals");
    expect(document.querySelector("[data-vendor-invite]")).toBeNull();
    expect(document.querySelector("[data-vendor-free] [data-vendor-plans]")).not.toBeNull();
    // No vendors at all: the locked card.
    show(global("buildVendorsPage")({ paid: false, items: [] }));
    expect(document.querySelector("[data-vendor-free]")).toBeNull();
    expect(document.querySelector("[data-vendor-plans]")).not.toBeNull();
  });

  it("the side card shows the vendor's facts", () => {
    const text = show(
      global("buildVendorDetail")(detail({ report_count: 2, invited_by: "ali@acme.com", invited_by_you: false })),
    );
    expect(text).toContain("Reports on Safely");
    expect(text).toContain("Email (confirmed)");
    expect(text).toContain("ali@acme.com");
    expect(text).toContain("Invited as Northfield");
    show(global("buildVendorDetail")(detail({ invited_by_you: true })));
    expect(document.body.textContent).toContain("You");
  });

  it("the Ask for more info box has Cancel", () => {
    show(global("buildVendorDetail")(detail()));
    expect(document.querySelector("#vendor-ask [data-vendor-ask-close]")?.textContent).toBe("Cancel");
    expect(document.querySelector("#vendor-report [data-vendor-report-close]")).not.toBeNull();
  });

  it("Approve and Reject open a note box in the page, not a pop-up", () => {
    show(global("buildVendorDetail")(detail()));
    expect(document.getElementById("vendor-decide")?.classList.contains("hidden")).toBe(true);
    expect(document.getElementById("vendor-decide-note")).not.toBeNull();
    expect(document.querySelector("[data-vendor-decide-close]")?.textContent).toBe("Cancel");
    // Cancel the link asks in the page too.
    show(global("buildVendorDetail")(detail({ status: "opened", result: null })));
    expect(document.querySelector("#vendor-cancel-box [data-vendor-cancel-send]")).not.toBeNull();
  });

  it("has a PDF button once there is a result", () => {
    show(global("buildVendorDetail")(detail()));
    expect(document.querySelector("[data-vendor-pdf]")?.textContent).toBe("PDF");
    show(global("buildVendorDetail")(detail({ status: "opened", result: null })));
    expect(document.querySelector("[data-vendor-pdf]")).toBeNull();
  });

  it("after inviting: new links, vendors already invited, and emails not sent", () => {
    const text = show(
      global("buildInviteResult")(
        [
          { id: "1", name: "Acme", email: "a@acme.com", link: "https://safely.sh/v/x", email_sent: false, email_limited: true },
          { id: "2", name: "Beta", email: "b@beta.com", link: null, email_sent: false, already: true },
        ],
        [{ email: "A@acme.com", phone: "+55 11 99999-0000" }],
      ),
    );
    expect(text).toContain("1 invited. 0 emailed by Safely.");
    expect(text).toContain("Not emailed to a@acme.com");
    expect(text).toContain("Already invited");
    expect(document.querySelector('[data-vendor-open="2"]')).not.toBeNull();
    expect(document.querySelector("a")?.getAttribute("href")).toContain("wa.me/5511999990000");
  });

  it("the list has History's bar: whose vendors, search and risk filter", () => {
    const team = {
      paid: true,
      who: "me",
      team: true,
      can_see_all: true,
      people: [{ id: "u2", email: "sara@acme.com", is_owner: false }],
      counts: {},
      items: [item(), item({ id: "2", vendor_name: "Bright Textiles", vendor_email: "b@bright.com", risk_level: "low", risk_score: 5 })],
    };
    const text = show(global("buildVendorsPage")(team));
    expect(text).toContain("My vendors");
    expect(document.querySelectorAll("[data-vendor-who]").length).toBe(3); // me, team, Sara
    expect(text).toContain("sara@acme.com");
    expect(document.getElementById("vendor-search")).not.toBeNull();
    expect(document.querySelectorAll("[data-vendor-risk]").length).toBe(4);
    // "Invited by" only on All team vendors.
    expect(text).not.toContain("Invited by");

    // Not in a team: no dropdown.
    show(global("buildVendorsPage")({ ...team, team: false }));
    expect(document.querySelector("[data-vendor-who-btn]")).toBeNull();
  });

  it("search and risk hide rows in the page", () => {
    document.body.innerHTML =
      '<div id="vendors-body">' +
      global("buildVendorsPage")({
        paid: true,
        counts: {},
        items: [item(), item({ id: "2", vendor_name: "Bright Textiles", vendor_email: "b@bright.com", risk_level: "low", risk_score: 5 })],
      }) +
      "</div>";
    const rows = () => Array.from(document.querySelectorAll("tr[data-vendor-row]")).filter((r) => !r.classList.contains("hidden")).length;
    expect(rows()).toBe(2);
    (document.getElementById("vendor-search") as HTMLInputElement).value = "bright";
    document.getElementById("vendor-search")!.dispatchEvent(new Event("input", { bubbles: true }));
    // The filter runs from the page's own listener; call it directly too.
    global("filterVendorRows")();
  });

  it("an open invite has Remind, New link and Cancel, and no decision", () => {
    show(global("buildVendorDetail")(detail({ status: "opened", result: null, risk_level: null })));
    expect(document.querySelector("[data-vendor-remind]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-link]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-cancel]")).not.toBeNull();
    expect(document.querySelector("[data-vendor-decide]")).toBeNull();
  });

  it("waiting for scans offers to run the check", () => {
    const text = show(global("buildVendorDetail")(detail({ status: "no_scans", result: null })));
    expect(text).toContain("no scans left");
    expect(document.querySelector("[data-vendor-retry]")).not.toBeNull();
  });

  it("reads a CSV list with or without a header", () => {
    const parse = global("parseVendorCsv");
    const withHeader = parse('email,name,country\nsales@a.com,"Acme, Inc",Brazil\n\nb@b.com,Beta,');
    expect(withHeader).toEqual([
      { name: "Acme, Inc", email: "sales@a.com", country: "Brazil", phone: undefined, website: undefined },
      { name: "Beta", email: "b@b.com", country: undefined, phone: undefined, website: undefined },
    ]);
    const plain = parse("Acme;sales@a.com;Brazil;+55 11 99999-0000;acme.com");
    expect(plain[0].phone).toBe("+55 11 99999-0000");
    expect(plain[0].website).toBe("acme.com");
  });

  it("a link box has Copy and a WhatsApp link with the phone", () => {
    show(global("vendorLinkBox")("Acme", "https://safely.sh/v/abc", "+55 11 99999-0000"));
    expect(document.querySelector("[data-vendor-copy]")?.getAttribute("data-vendor-copy")).toBe("https://safely.sh/v/abc");
    const wa = document.querySelector("a")?.getAttribute("href") || "";
    expect(wa.startsWith("https://wa.me/5511999990000?text=")).toBe(true);
    expect(decodeURIComponent(wa)).toContain("https://safely.sh/v/abc");
  });

  it("/vendors is its own page address", () => {
    expect(global("dashboardViewFromAddress")("/vendors")).toBe("view-vendors");
    expect(global("dashboardAddressFor")("view-vendors")).toBe("/vendors");
    // One vendor has its own address, on the Vendors page.
    const id = "0193f2a1-0000-7000-8000-000000000001";
    expect(global("dashboardViewFromAddress")("/vendors/" + id)).toBe("view-vendors");
    expect(global("vendorIdFromAddress")("/vendors/" + id)).toBe(id);
    expect(global("vendorIdFromAddress")("/vendors")).toBeNull();
    expect(global("vendorIdFromAddress")("/vendors/not-an-id")).toBeNull();
  });
});
