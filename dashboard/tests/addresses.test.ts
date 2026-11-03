import { describe, it, expect, beforeAll, vi } from "vitest";
import { loadScripts, global, readText } from "./load-script";

// Each dashboard page has its own address: /dashboard is History,
// /reports is My Reports, /settings is Settings, and one result is
// /dashboard/<platform>/<scan id>. A refresh stays on the page, and
// Back / Forward move between them (ts/shared.ts, ts/detail_view.ts).

const ID = "0193f2a1-0000-7000-8000-000000000001";

const RESULT = {
  listing_title: "Acme",
  listing_url: "https://www.alibaba.com/product/1",
  risk_score: 40,
  risk_level: "caution",
  fraud_report_count: 0,
  platform: "alibaba",
  seller: {
    name: "Acme",
    handle: null,
    phone: null,
    account_age: null,
    location: null,
    last_active: null,
    verification: "unknown",
    monthly_activity: [],
    network_summary: null,
  },
  signals: [],
  risk_factors: [],
  reports: [],
  social_candidates: [],
};

const DETAIL_IDS = [
  "detail-loading", "detail-body", "detail-title", "detail-listing-link-text", "detail-verdict",
  "detail-findings", "detail-seller-name", "detail-seller-location", "detail-facts", "detail-visits",
  "detail-tab-content-risk", "detail-tab-content-intel", "detail-tab-content-report",
  "detail-report-filed", "detail-report-empty",
];

const checked = () => (document.querySelector('input[name="view"]:checked') as HTMLInputElement).id;
const resultOpen = () => !document.getElementById("detail-view")!.classList.contains("hidden");
const click = (id: string) => (document.getElementById(id) as HTMLInputElement).click();
const back = (path: string) => {
  history.replaceState(null, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
};

beforeAll(async () => {
  // Opened at /settings, as after a refresh there.
  history.replaceState(null, "", "/settings");
  document.body.innerHTML =
    '<input type="radio" name="view" id="view-history" checked>' +
    '<input type="radio" name="view" id="view-reports">' +
    '<input type="radio" name="view" id="view-settings">' +
    '<div id="detail-view" class="hidden"><a id="detail-listing-link"></a>' +
    DETAIL_IDS.map((id) => '<div id="' + id + '"></div>').join("") +
    "</div>";
  (window as any).safelyAuth = { authHeader: () => ({}), logout: vi.fn() };
  (globalThis as any).fetch = vi.fn(async () => ({ status: 200, ok: true, json: async () => RESULT }));
  await loadScripts(["ts/shared.ts", "ts/detail_view.ts"]);
  // As init.ts does: switching page closes an open result.
  ["view-history", "view-reports", "view-settings"].forEach((id) =>
    document.getElementById(id)!.addEventListener("change", global("closeDetailPanel")),
  );
});

describe("addresses", () => {
  it("names each page and each result", () => {
    expect(global("dashboardAddressFor")("view-history")).toBe("/dashboard/");
    expect(global("dashboardAddressFor")("view-reports")).toBe("/reports");
    expect(global("dashboardAddressFor")("view-settings")).toBe("/settings");
    expect(global("resultAddressFor")("alibaba", ID)).toBe("/dashboard/alibaba/" + ID);
    expect(global("resultAddressFor")("website", ID)).toBe("/dashboard/website/" + ID);
  });

  it("reads the page and the result back from an address", () => {
    const view = global("dashboardViewFromAddress");
    expect(view("/settings")).toBe("view-settings");
    expect(view("/settings/")).toBe("view-settings");
    expect(view("/reports")).toBe("view-reports");
    expect(view("/dashboard/")).toBe("view-history");
    expect(view("/dashboard/alibaba/" + ID)).toBe("view-history");
    const id = global("resultIdFromAddress");
    expect(id("/dashboard/kompass/" + ID)).toBe(ID);
    expect(id("/dashboard/alibaba/not-a-scan")).toBeNull();
    expect(id("/settings")).toBeNull();
  });
});

describe("pages, refresh and Back", () => {
  it("opens on the page in the address (a refresh on /settings stays there)", () => {
    expect(checked()).toBe("view-settings");
  });

  it("clicking a page gives it its own address", () => {
    click("view-reports");
    expect(location.pathname).toBe("/reports");
    click("view-history");
    expect(location.pathname).toBe("/dashboard/");
  });

  it("Back shows the page of the address, and lets its own code know", () => {
    const loaded = vi.fn();
    document.getElementById("view-settings")!.addEventListener("change", loaded);
    back("/settings");
    expect(checked()).toBe("view-settings");
    expect(loaded).toHaveBeenCalledTimes(1);
    expect(location.pathname).toBe("/settings");
  });
});

describe("a result's own address", () => {
  it("opening a result gives it its address; closing goes back to History", async () => {
    click("view-history");
    await global("openDetail")(ID);
    expect(resultOpen()).toBe(true);
    expect(location.pathname).toBe("/dashboard/alibaba/" + ID);
    global("closeDetailPanel")();
    expect(resultOpen()).toBe(false);
    expect(location.pathname).toBe("/dashboard/");
  });

  it("Back to a result opens it again, and Back from it closes it", async () => {
    back("/dashboard/alibaba/" + ID);
    await vi.waitFor(() => expect(resultOpen()).toBe(true));
    back("/dashboard/");
    expect(resultOpen()).toBe(false);
    expect(location.pathname).toBe("/dashboard/");
  });

  // Real bug (Nov 3): Back from Settings to a result landed on History.
  it("Back from Settings to a result keeps the result's address", async () => {
    back("/settings");
    expect(checked()).toBe("view-settings");
    back("/dashboard/alibaba/" + ID);
    await vi.waitFor(() => expect(resultOpen()).toBe(true));
    expect(checked()).toBe("view-history");
    expect(location.pathname).toBe("/dashboard/alibaba/" + ID);
  });
});

describe("index.html", () => {
  // /settings and /dashboard/alibaba/<id> are not in the dashboard
  // folder, so a path like "js/init.js" would be looked for elsewhere.
  it("loads every file of its own from /dashboard/", async () => {
    const page = await readText("index.html");
    expect(page).not.toMatch(/(src|href)="(js|css|images|locales)\//);
    expect(page).toContain('src="/dashboard/js/init.js"');
    expect(page).toContain('href="/dashboard/css/dashboard.css"');
  });
});
