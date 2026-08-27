import { describe, it, expect, vi, beforeEach } from "vitest";
import { fakeChrome } from "./setup-chrome";

fakeChrome.runtime.sendMessage = vi.fn();

import "../ts/auth-bridge";

const relayToken = (globalThis as any).__safelyRelayToken;
const resetLastSent = (globalThis as any).__safelyResetLastSent;

beforeEach(() => {
  vi.clearAllMocks();
  resetLastSent();
  window.localStorage.clear();
});

describe("relayToken", () => {
  it("sends the real token when one exists in localStorage", () => {
    window.localStorage.setItem("safely_session_token", "real-token-abc");
    relayToken();

    expect(fakeChrome.runtime.sendMessage).toHaveBeenCalledWith({
      type: "SAFELY_SESSION_UPDATE",
      token: "real-token-abc",
    });
  });

  it("sends null when no token exists at all", () => {
    relayToken();

    expect(fakeChrome.runtime.sendMessage).toHaveBeenCalledWith({
      type: "SAFELY_SESSION_UPDATE",
      token: null,
    });
  });

  it("does NOT send again if the token genuinely hasn't changed", () => {
    window.localStorage.setItem("safely_session_token", "same-token");
    relayToken();
    relayToken();

    // Only the first call should have actually sent a message - the
    // second call sees the identical token and correctly stays quiet.
    expect(fakeChrome.runtime.sendMessage).toHaveBeenCalledTimes(1);
  });

  it("sends again once the token genuinely changes", () => {
    window.localStorage.setItem("safely_session_token", "token-one");
    relayToken();

    window.localStorage.setItem("safely_session_token", "token-two");
    relayToken();

    expect(fakeChrome.runtime.sendMessage).toHaveBeenCalledTimes(2);
    expect(fakeChrome.runtime.sendMessage).toHaveBeenLastCalledWith({
      type: "SAFELY_SESSION_UPDATE",
      token: "token-two",
    });
  });

  it("sends again once the token disappears (logout)", () => {
    window.localStorage.setItem("safely_session_token", "token-one");
    relayToken();

    window.localStorage.removeItem("safely_session_token");
    relayToken();

    expect(fakeChrome.runtime.sendMessage).toHaveBeenCalledTimes(2);
    expect(fakeChrome.runtime.sendMessage).toHaveBeenLastCalledWith({
      type: "SAFELY_SESSION_UPDATE",
      token: null,
    });
  });
});
