import { describe, it, expect, beforeAll } from "vitest";
import { loadScript, global } from "./load-script";

// The numbers above the history table, counted from the rows the
// server sends (backend/templates/history_rows.html). Loads the real
// ts/history.ts.

beforeAll(async () => {
  await loadScript("ts/shared.ts");
  await loadScript("ts/history.ts");
});

function page(rows: string, reports = ""): void {
  document.body.innerHTML =
    '<b id="stat-checked-num"></b><b id="stat-high-num"></b><b id="stat-reported-num"></b>' +
    '<table><tbody id="history-rows">' +
    rows +
    '</tbody></table><table><tbody id="report-rows">' +
    reports +
    "</tbody></table>";
}

const stat = (id: string) => document.getElementById(id)?.textContent;

describe("history numbers", () => {
  it("counts checked, high-risk and reported", () => {
    page(
      '<tr data-id="1" data-risk="high"></tr>' +
        '<tr data-id="2" data-risk="caution"></tr>' +
        '<tr data-id="3" data-risk="high"></tr>',
      '<tr data-report="a"></tr>',
    );
    global("renderStats")();
    expect(stat("stat-checked-num")).toBe("3");
    expect(stat("stat-high-num")).toBe("2");
    expect(stat("stat-reported-num")).toBe("1");
  });

  it("shows 0 for the empty 'No scans yet' row", () => {
    page(
      '<tr><td colspan="4">No scans yet. Open a supplier or listing page with the Safely extension to get started.</td></tr>',
    );
    global("renderStats")();
    expect(stat("stat-checked-num")).toBe("0");
    expect(stat("stat-high-num")).toBe("0");
  });
});
