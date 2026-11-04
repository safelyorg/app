import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeChrome } from "./setup-chrome";
import "../ts/scrapers/index";
import "../ts/background";
import "../ts/core/api";

// A scan is sent on a port to the background script. When the buyer
// leaves (another listing, another website, the tab closed), the port
// closes and the call to Safely is stopped - Safely's server then gives
// the scan back, so a result nobody saw is not counted.

type Listener = (arg?: any) => void;

// One end of a chrome.runtime port.
function fakePort(name = "safely-scan") {
  const onMessage: Listener[] = [];
  const onDisconnect: Listener[] = [];
  return {
    name,
    sender: { id: "safely-extension" },
    posted: [] as any[],
    disconnected: false,
    onMessage: { addListener: (f: Listener) => onMessage.push(f) },
    onDisconnect: { addListener: (f: Listener) => onDisconnect.push(f) },
    postMessage(m: any) {
      this.posted.push(m);
    },
    disconnect() {
      this.disconnected = true;
    },
    // The other end answers / goes away.
    reply(m: any) {
      onMessage.forEach((f) => f(m));
    },
    drop() {
      onDisconnect.forEach((f) => f());
    },
  };
}

const scanFetch = (window as any).__safelyScanFetch;
const handleScanPort = (globalThis as any).__safelyHandleScanPort;
const api = (window as any).__safelyAPI;

beforeEach(() => {
  (fakeChrome.runtime as any).id = "safely-extension";
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("the page's side (safelyScanFetch)", () => {
  it("sends the call on a port and gives back the server's answer", async () => {
    const port = fakePort();
    (fakeChrome.runtime as any).connect = vi.fn(() => port);
    const answer = scanFetch(
      "http://localhost:3000/api/v1/check-website",
      { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" },
      () => true,
    );
    expect(port.posted[0]).toMatchObject({
      type: "SAFELY_API_REQUEST",
      url: "http://localhost:3000/api/v1/check-website",
      method: "POST",
      body: "{}",
    });
    port.reply({ status: 200, text: '{"risk_score":12}' });
    const response = await answer;
    expect(response.status).toBe(200);
    expect(await response.text()).toBe('{"risk_score":12}');
  });

  it("stops the call when the buyer has left, and never asks again", async () => {
    vi.useFakeTimers();
    const port = fakePort();
    (fakeChrome.runtime as any).connect = vi.fn(() => port);
    let stillHere = true;
    const answer = scanFetch("http://localhost:3000/api/v1/analyze", { method: "POST" }, () => stillHere);
    const failed = expect(answer).rejects.toThrow("cancelled");
    stillHere = false;
    await vi.advanceTimersByTimeAsync(600);
    await failed;
    expect(port.disconnected).toBe(true);
    expect(fakeChrome.runtime.connect).toHaveBeenCalledTimes(1);
  });

  it("fails like a network error when the background script stops", async () => {
    const port = fakePort();
    (fakeChrome.runtime as any).connect = vi.fn(() => port);
    const answer = scanFetch("http://localhost:3000/api/v1/analyze", { method: "POST" }, () => true);
    port.drop();
    await expect(answer).rejects.toThrow("Failed to fetch");
  });
});

describe("the background script's side (handleScanPort)", () => {
  it("calls Safely and answers on the port", async () => {
    const port = fakePort();
    (globalThis as any).fetch = vi.fn().mockResolvedValue({ status: 200, text: async () => "ok" });
    handleScanPort(port);
    port.reply({ type: "SAFELY_API_REQUEST", url: "https://safely.sh/api/v1/analyze", method: "POST", headers: {}, body: "{}" });
    await vi.waitFor(() => expect(port.posted).toEqual([{ status: 200, text: "ok" }]));
  });

  it("stops the call to Safely when the page goes away", async () => {
    const port = fakePort();
    let signal: AbortSignal | undefined;
    (globalThis as any).fetch = vi.fn((_url: string, init: RequestInit) => {
      signal = init.signal as AbortSignal;
      return new Promise(() => {}); // the server is still working
    });
    handleScanPort(port);
    port.reply({ type: "SAFELY_API_REQUEST", url: "https://safely.sh/api/v1/analyze", method: "POST", headers: {}, body: "{}" });
    expect(signal!.aborted).toBe(false);
    port.drop();
    expect(signal!.aborted).toBe(true);
  });

  it("refuses other addresses, other ports and other senders", () => {
    (globalThis as any).fetch = vi.fn();
    const port = fakePort();
    handleScanPort(port);
    port.reply({ type: "SAFELY_API_REQUEST", url: "https://evil.example/steal", method: "GET", headers: {}, body: null });
    expect(port.posted).toEqual([{ networkError: "not allowed" }]);

    const stranger = fakePort();
    stranger.sender = { id: "another-extension" };
    handleScanPort(stranger);
    expect(stranger.disconnected).toBe(true);

    const other = fakePort("something-else");
    handleScanPort(other);
    other.reply({ type: "SAFELY_API_REQUEST", url: "https://safely.sh/api/v1/analyze" });
    expect(other.posted).toEqual([]);
    expect((globalThis as any).fetch).not.toHaveBeenCalled();
  });
});

describe("api.ts sends scans this way", () => {
  it("analyze and analyzeWebsite pass 'is the buyer still here?' along", async () => {
    fakeChrome.storage.local.get = vi.fn().mockResolvedValue({});
    const calls: any[] = [];
    (window as any).__safelyScanFetch = vi.fn(async (url: string, _init: RequestInit, stillWanted: () => boolean) => {
      calls.push({ url, wanted: stillWanted() });
      return { ok: true, status: 200, text: async () => '{"risk_score":5}' };
    });
    const stillHere = () => false;
    await api.analyze({} as any, stillHere);
    await api.analyzeWebsite({ url: "https://shop.com/", language: "en", pages: [] }, stillHere);
    expect(calls.map((c) => c.url)).toEqual([
      "http://localhost:3000/api/v1/analyze",
      "http://localhost:3000/api/v1/check-website",
    ]);
    expect(calls.every((c) => c.wanted === false)).toBe(true);
    (window as any).__safelyScanFetch = scanFetch;
  });
});
