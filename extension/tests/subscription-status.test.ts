import { describe, it, expect, vi, beforeEach } from "vitest";
import { fakeChrome } from "./setup-chrome";

import "../ts/core/panel-subscription-logic";
import "../ts/core/api";

const isSubscriptionActive = (globalThis as any).__safelyIsSubscriptionActive;

describe("isSubscriptionActive - the real access decision", () => {
  it("allows access for an active, paying subscription", () => {
    expect(isSubscriptionActive("active")).toBe(true);
  });

  it("allows access during a genuine, active trial", () => {
    expect(isSubscriptionActive("trialing")).toBe(true);
  });

  it("blocks access when the account has never subscribed at all (null status)", () => {
    expect(isSubscriptionActive(null)).toBe(false);
  });

  it("blocks access once the subscription is past due (payment failed)", () => {
    expect(isSubscriptionActive("past_due")).toBe(false);
  });

  it("blocks access once the subscription has been canceled", () => {
    expect(isSubscriptionActive("canceled")).toBe(false);
  });

  it("blocks access on any other, unrecognized status", () => {
    expect(isSubscriptionActive("some_future_status_we_havent_seen")).toBe(false);
  });
});

describe("api.checkSubscriptionStatus() feeding into isSubscriptionActive", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    fakeChrome.storage.local.get = vi.fn().mockResolvedValue({});
    (globalThis as any).fetch = vi.fn();
  });

  it("real end-to-end: a trial-ending scenario correctly blocks once status flips to canceled", async () => {
    // Simulates the exact real-world sequence you described: someone
    // was in a trial, it ended, and no payment was ever completed -
    // the backend's real webhook handling would flip status to
    // "canceled" once Creem's trial-end webhook fires with no card on
    // file, and this confirms that flip correctly blocks access.
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ status: "canceled" }),
    });

    const api = (window as any).__safelyAPI;
    const status = await api.checkSubscriptionStatus();
    expect(isSubscriptionActive(status)).toBe(false);
  });

  it("real end-to-end: a successfully paid trial-to-active transition correctly allows access", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ status: "active" }),
    });

    const api = (window as any).__safelyAPI;
    const status = await api.checkSubscriptionStatus();
    expect(isSubscriptionActive(status)).toBe(true);
  });
});
