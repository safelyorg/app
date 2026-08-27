import { describe, it, expect, vi, beforeEach } from "vitest";
import { fakeChrome } from "./setup-chrome";

import "../ts/background";

const handleMessage = (globalThis as any).__safelyHandleSessionUpdateMessage;

beforeEach(() => {
  vi.clearAllMocks();
});

describe("handleSessionUpdateMessage", () => {
  it("stores the token when a real, non-null token is received", () => {
    const sendResponse = vi.fn();
    handleMessage({ type: "SAFELY_SESSION_UPDATE", token: "abc123" }, {}, sendResponse);

    expect(fakeChrome.storage.local.set).toHaveBeenCalledWith({
      safely_session_token: "abc123",
    });
    expect(fakeChrome.storage.local.remove).not.toHaveBeenCalled();
    expect(sendResponse).toHaveBeenCalledWith({ status: "ok" });
  });

  it("removes the stored token when the token is null (logged out)", () => {
    const sendResponse = vi.fn();
    handleMessage({ type: "SAFELY_SESSION_UPDATE", token: null }, {}, sendResponse);

    expect(fakeChrome.storage.local.remove).toHaveBeenCalledWith("safely_session_token");
    expect(fakeChrome.storage.local.set).not.toHaveBeenCalled();
    expect(sendResponse).toHaveBeenCalledWith({ status: "ok" });
  });

  it("ignores messages with a genuinely different type", () => {
    const sendResponse = vi.fn();
    handleMessage({ type: "SOMETHING_ELSE" } as any, {}, sendResponse);

    expect(fakeChrome.storage.local.set).not.toHaveBeenCalled();
    expect(fakeChrome.storage.local.remove).not.toHaveBeenCalled();
    expect(sendResponse).not.toHaveBeenCalled();
  });

  it("always returns true, keeping the message channel open for async response", () => {
    const result = handleMessage(
      { type: "SAFELY_SESSION_UPDATE", token: "abc" },
      {},
      vi.fn(),
    );
    expect(result).toBe(true);
  });
});
