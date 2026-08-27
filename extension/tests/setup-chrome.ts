import { vi } from "vitest";

export const fakeChrome = {
  runtime: {
    onInstalled: { addListener: vi.fn() },
    onMessage: { addListener: vi.fn() },
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
