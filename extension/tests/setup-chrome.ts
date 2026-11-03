import { vi } from "vitest";

export const fakeChrome = {
  runtime: {
    onInstalled: { addListener: vi.fn() },
    onMessage: { addListener: vi.fn() },
    // background.ts listens for scans sent on a port (handleScanPort).
    onConnect: { addListener: vi.fn() },
    // A scan opens a port to the background script (safelyScanFetch in
    // scrapers/index.ts); the tests in scan-stop.test.ts set what it returns.
    connect: vi.fn(),
    sendMessage: vi.fn(),
  },
  storage: {
    local: {
      set: vi.fn(),
      remove: vi.fn(),
      get: vi.fn().mockResolvedValue({}),
    },
  },
};

(globalThis as any).chrome = fakeChrome;
