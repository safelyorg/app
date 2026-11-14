import { describe, it, expect, beforeAll } from "vitest";
import { loadScripts, global } from "./load-script";

// Extra seats on the owner's Team card (ts/team.ts): the -/+ counter is
// always there and picks how many. "Add n" opens the payment page for n
// seats ($25 each). "Remove n" shows once there are extra seats and takes
// away n free seats (no refund). The 3 plan seats can't be removed, nor a
// seat someone is using.

beforeAll(async () => {
  await loadScripts(["ts/shared.ts", "ts/team.ts"]);
});

function card(extra: number, used: number): HTMLElement {
  const box = document.createElement("div");
  global("renderOwnerView")(box, {
    plan: "Team",
    seat_limit: 3 + extra,
    seats_used: used,
    members: [],
    extra_seats: extra,
    can_buy_seats: true,
    seat_price_usd: 25,
    max_extra_seats: 50,
  });
  return box;
}

const q = (box: HTMLElement, sel: string) => box.querySelector(sel) as HTMLButtonElement;

describe("extra seats", () => {
  it("shows the plan seats, that they can't be removed, and no refund", () => {
    const text = card(2, 3).textContent || "";
    expect(text).toContain("Seats: 5 (3 in your plan + 2 extra)");
    expect(text).toContain("can't be removed");
    expect(text).toContain("(no refund)");
  });

  it("no extra seats yet: the counter and Add, no Remove, no hint", () => {
    const box = card(0, 1);
    expect(q(box, "[data-seat-count]")).not.toBeNull();
    expect(q(box, "[data-seat-remove]")).toBeNull();
    expect(q(box, "[data-seat-hint]").textContent).toBe("");
    q(box, "[data-seat-plus]").click();
    q(box, "[data-seat-plus]").click();
    expect(q(box, "[data-seat-count]").textContent).toBe("3");
    expect(q(box, "[data-seat-add]").textContent).toBe("Add 3");
    q(box, "[data-seat-add]").click();
    const text = q(box, "[data-seat-confirm]").textContent || "";
    expect(text).toContain("Add 3 seats for $75 a month ($25 each)");
    expect(text).toContain("payment page");
  });

  it("with 1 extra seat the counter still goes up (to add more)", () => {
    const box = card(1, 1);
    expect(q(box, "[data-seat-plus]").disabled).toBe(false);
    q(box, "[data-seat-plus]").click();
    expect(q(box, "[data-seat-count]").textContent).toBe("2");
    expect(q(box, "[data-seat-add]").disabled).toBe(false);
    // Only 1 extra seat: Remove 2 is off.
    expect(q(box, "[data-seat-remove]").disabled).toBe(true);
  });

  it("removing seats: removed now, no refund", () => {
    const box = card(3, 3);
    q(box, "[data-seat-remove]").click();
    let text = q(box, "[data-seat-confirm]").textContent || "";
    expect(text).toContain("Remove 1 seat? Removed now");
    expect(text).toContain("not refunded");
    q(box, "[data-seat-plus]").click();
    expect(q(box, "[data-seat-remove]").textContent).toBe("Remove 2");
    q(box, "[data-seat-remove]").click();
    text = q(box, "[data-seat-confirm]").textContent || "";
    expect(text).toContain("Remove 2 seats? Removed now");
  });

  it("only free extra seats can be removed", () => {
    // 2 extra seats, 4 of 5 seats used: 1 can be removed.
    const box = card(2, 4);
    expect(q(box, "[data-seat-remove]").disabled).toBe(false);
    q(box, "[data-seat-plus]").click();
    expect(q(box, "[data-seat-remove]").disabled).toBe(true);
    expect(q(box, "[data-seat-hint]").textContent).toContain("up to 1");
  });

  it("every seat in use: Remove is off", () => {
    const box = card(1, 4);
    expect(q(box, "[data-seat-remove]").disabled).toBe(true);
    expect(q(box, "[data-seat-hint]").textContent).toContain("Remove a person first");
  });
});
