import { describe, it, expect, beforeAll } from "vitest";
import { loadScripts, global } from "./load-script";

// The "Watched suppliers" page (ts/watch.ts).

beforeAll(async () => {
  await loadScripts(["ts/shared.ts", "ts/watch.ts"]);
});

const item = (over: Record<string, unknown> = {}) => ({
  seller_id: "s1",
  analysis_id: "a1",
  name: "Bernal",
  platform: "thomasnet",
  platform_name: "ThomasNet",
  status: "no_change",
  changes: [],
  listing_checked: true,
  website: null,
  watched_since: "2026-11-10",
  last_checked: "2026-11-13",
  ...over,
});

function page(list: Record<string, unknown>): string {
  document.body.innerHTML = global("buildWatchPage")(list);
  return document.body.textContent || "";
}

describe("watched suppliers page", () => {
  it("Free: a locked card with See plans", () => {
    const text = page({ paid: false, limit: 0, used: 0, every_days: null, next_check: null, items: [] });
    expect(text).toContain("Team and Enterprise");
    expect(document.querySelector("[data-watch-plans]")).not.toBeNull();
  });

  it("Team: used out of 50, and an empty list", () => {
    const text = page({ paid: true, limit: 25, used: 0, every_days: 7, next_check: null, items: [] });
    expect(text).toContain("0 / 25");
    expect(text).toContain("No suppliers watched yet");
  });

  it("shows what changed, and Open / Stop for each supplier", () => {
    const text = page({
      paid: true,
      limit: null,
      used: 2,
      every_days: 1,
      next_check: "2026-11-14",
      items: [
        item({
          added_by: "ali@company.com",
          added_by_name: "Ali Khan",
          status: "changed",
          changes: [{ kind: "scam_report", title: "New scam report", detail: "Safely users reported it." }],
        }),
        item({ seller_id: "s2", status: "listing_not_checked", added_by: "sara@company.com" }),
      ],
    });
    expect(text).toContain("New scam report");
    expect(text).toContain("Listing not checked");
    expect(text).toContain("ThomasNet");
    expect(text).toContain("Added by");
    expect(text).toContain("Ali Khan");
    // No name: the email.
    expect(text).toContain("sara@company.com");
    expect(document.querySelectorAll("[data-watch-stop]").length).toBe(2);
    expect(document.querySelector('[data-watch-open="a1"]')).not.toBeNull();
  });

  it("supplier names are text, never HTML", () => {
    page({
      paid: true,
      limit: 25,
      used: 1,
      every_days: 7,
      next_check: null,
      items: [item({ name: "<img src=x onerror=alert(1)>" })],
    });
    expect(document.querySelector("img")).toBeNull();
  });
});
