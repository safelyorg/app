import { describe, it, expect, beforeAll } from "vitest";
import { loadScripts, global } from "./load-script";

// The owner's Team card on the Free plan (ts/team.ts): "Only you", or
// after a team plan ended, the people still listed and no invite box.

beforeAll(async () => {
  await loadScripts(["ts/shared.ts", "ts/team.ts"]);
});

function card(data: Record<string, unknown>): HTMLElement {
  const box = document.createElement("div");
  global("renderOwnerView")(box, data);
  return box;
}

describe("owner Team card", () => {
  it("Free, nobody invited: Only you", () => {
    const box = card({ plan: "Free", seat_limit: 1, seats_used: 1, members: [] });
    expect(box.textContent).toContain("Only you");
    expect(box.textContent).not.toContain("seats used");
  });

  it("Free after the plan ended: the people, no seat count, no invite box", () => {
    const box = card({
      plan: "Free",
      seat_limit: 1,
      seats_used: 2,
      members: [{ email: "ali@company.com", joined: true, has_seat: false }],
    });
    const text = box.textContent || "";
    expect(text).toContain("Your team plan has ended");
    expect(text).toContain("ali@company.com");
    expect(text).toContain("On the Free plan");
    expect(text).not.toContain("2 of 1");
    expect(box.querySelector("form")).toBeNull();
  });

  it("Team: seats used and the invite box", () => {
    const box = card({
      plan: "Team",
      seat_limit: 3,
      seats_used: 2,
      members: [{ email: "ali@company.com", joined: true, has_seat: true }],
    });
    expect(box.textContent).toContain("2 of 3 seats used");
    expect(box.querySelector("form")).not.toBeNull();
  });
});
