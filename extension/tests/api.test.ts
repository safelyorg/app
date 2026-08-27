import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeChrome } from "./setup-chrome";

import "../ts/core/api";

const api = (window as any).__safelyAPI;

let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.clearAllMocks();
  fakeChrome.storage.local.get = vi.fn().mockResolvedValue({});
  (globalThis as any).fetch = vi.fn();
  consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  consoleErrorSpy.mockRestore();
});

describe("analyze", () => {
  it("returns the parsed response on success", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ risk_score: 42, fraud_report_count: 0 }),
    });

    const result = await api.analyze({} as any);
    expect(result).toEqual({ risk_score: 42, fraud_report_count: 0 });
  });

  it("attaches the real Authorization header when a session token exists", async () => {
    fakeChrome.storage.local.get = vi.fn().mockResolvedValue({
      safely_session_token: "real-token-123",
    });
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ risk_score: 10 }),
    });

    await api.analyze({} as any);

    const callArgs = (globalThis as any).fetch.mock.calls[0];
    expect(callArgs[1].headers.Authorization).toBe("Bearer real-token-123");
  });

  it("does not attach an Authorization header when no token exists", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ risk_score: 10 }),
    });

    await api.analyze({} as any);

    const callArgs = (globalThis as any).fetch.mock.calls[0];
    expect(callArgs[1].headers.Authorization).toBeUndefined();
  });

  it("returns a rate_limited error with the real retry time on a 429 response", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: false,
      status: 429,
      text: async () => "error: RATE_LIMITED:45",
    });

    const result = await api.analyze({} as any);
    expect(result).toEqual({ error: "rate_limited", retryAfterSeconds: 45 });
  });

  it("returns an unauthorized error on a 401 response", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => "error: unauthorized",
    });

    const result = await api.analyze({} as any);
    expect(result).toEqual({ error: "unauthorized" });
  });

  it("returns null for any other, unrecognized failure", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: false,
      status: 500,
      text: async () => "error: something else broke",
    });

    const result = await api.analyze({} as any);
    expect(result).toBeNull();
  });

  it("returns null when the fetch call itself throws (e.g. network failure)", async () => {
    (globalThis as any).fetch.mockRejectedValue(new Error("network down"));

    const result = await api.analyze({} as any);
    expect(result).toBeNull();
  });
});

describe("submitReport", () => {
  it("returns the parsed response on success", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ success: true }),
    });

    const result = await api.submitReport({} as any);
    expect(result).toEqual({ success: true });
  });

  it("returns an unauthorized error on a 401 response", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => "error: unauthorized",
    });

    const result = await api.submitReport({} as any);
    expect(result).toEqual({ error: "unauthorized" });
  });

  it("returns null for any other failure", async () => {
    (globalThis as any).fetch.mockResolvedValue({
      ok: false,
      status: 500,
      text: async () => "error: server broke",
    });

    const result = await api.submitReport({} as any);
    expect(result).toBeNull();
  });

  it("returns null when the fetch call itself throws", async () => {
    (globalThis as any).fetch.mockRejectedValue(new Error("network down"));

    const result = await api.submitReport({} as any);
    expect(result).toBeNull();
  });
});
