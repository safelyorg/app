import { describe, it, expect, beforeAll } from "vitest";
import { loadScripts, global } from "./load-script";

// The History dropdown for teams (ts/team_history.ts): the owner gets
// "All team scans", a member picks one teammate at a time.

beforeAll(async () => {
  await loadScripts(["ts/shared.ts", "ts/team_history.ts", "ts/history.ts"]);
});

const owner = {
  team: true,
  can_see_all: true,
  people: [
    { id: "a1", email: "ali@company.com", is_owner: false },
    { id: "s1", email: "sara@company.com", is_owner: false },
  ],
};

const member = {
  team: true,
  can_see_all: false,
  people: [
    { id: "o1", email: "bilal@company.com", is_owner: true },
    { id: "s1", email: "sara@company.com", is_owner: false },
  ],
};

function choicesIn(html: string): string[] {
  document.body.innerHTML = html;
  return [...document.querySelectorAll("[data-history-who]")].map(
    (el) => (el as HTMLElement).dataset.historyWho as string,
  );
}

describe("team History dropdown", () => {
  it("owner: My scans, All team scans, then each member", () => {
    expect(choicesIn(global("buildHistoryDropdown")(owner))).toEqual(["me", "team", "a1", "s1"]);
    expect(document.body.textContent).toContain("Team members");
  });

  it("member: no All team scans, the owner is marked", () => {
    expect(choicesIn(global("buildHistoryDropdown")(member))).toEqual(["me", "o1", "s1"]);
    expect(document.body.textContent).toContain("bilal@company.com (owner)");
  });

  it("short name for the labels", () => {
    expect(global("shortName")("ali@company.com")).toBe("ali");
  });

  it("emails are shown as text, never as HTML", () => {
    const html = global("buildHistoryDropdown")({
      team: true,
      can_see_all: true,
      people: [{ id: "x", email: "<img src=x onerror=alert(1)>@a.com", is_owner: false }],
    });
    document.body.innerHTML = html;
    expect(document.querySelector("img")).toBeNull();
  });

  it("My scans counts your reports", () => {
    document.body.innerHTML =
      '<b id="stat-checked-num"></b><b id="stat-high-num"></b><b id="stat-reported-num"></b>' +
      '<table><tbody id="history-rows"><tr data-id="1" data-risk="low" data-reported="true"></tr></tbody></table>' +
      '<table><tbody id="report-rows"></tbody></table>';
    global("renderStats")();
    expect(document.getElementById("stat-reported-num")?.textContent).toBe("0");
  });
});
