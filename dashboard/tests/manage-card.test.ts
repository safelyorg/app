import { describe, it, expect, beforeAll } from "vitest";
import { readText } from "./load-script";

// "Manage card" in Plan & Billing: one button (index.html) that asks the
// server for Creem's card page and opens it (ts/init.ts).

let html = "";
let init = "";

beforeAll(async () => {
  html = await readText("index.html");
  init = await readText("ts/init.ts");
});

describe("Manage card", () => {
  it("has one button, hidden until a paid plan is shown", () => {
    expect(html.match(/id="manage-card-btn"/g)?.length).toBe(1);
    expect(html).toMatch(/id="manage-card-area" class="hidden/);
  });

  it("opens the link the server gets from Creem", () => {
    expect(init).toContain('API_BASE + "/billing/manage-card"');
    expect(init).toContain("window.location.href = data.url");
    expect(init).toContain('manageCardArea.classList.toggle("hidden", !isPaid)');
  });
});
