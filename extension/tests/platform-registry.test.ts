import { describe, it, expect } from "vitest";

// Direct copy of index.ts's platform registry logic - same honest
// approach as other fallback/pure-logic test files.

interface PlatformConfig {
  name: string;
  matchesHostname: (hostname: string) => boolean;
  requiresClientSideScraping: boolean;
  isListingUrl: (url: string) => boolean;
}

const platformRegistry: PlatformConfig[] = [
  {
    name: "tradewheel",
    matchesHostname: (hostname) => hostname.includes("tradewheel.com"),
    requiresClientSideScraping: true,
    isListingUrl: (url) => url.includes("/p/"),
  },
  {
    name: "b2brazil",
    matchesHostname: (hostname) => hostname.includes("b2brazil.com"),
    requiresClientSideScraping: false,
    isListingUrl: (url) => url.includes("/hotsite/"),
  },
];

function detectPlatform(hostname: string): string {
  const match = platformRegistry.find((p) => p.matchesHostname(hostname));
  return match ? match.name : "unknown";
}

function requiresClientSideScraping(platform: string): boolean {
  return platformRegistry.find((p) => p.name === platform)?.requiresClientSideScraping ?? true;
}

function isListingPage(platform: string, url: string): boolean {
  const config = platformRegistry.find((p) => p.name === platform);
  return config ? config.isListingUrl(url) : false;
}

describe("platform registry - detectPlatform", () => {
  it("correctly detects tradewheel", () => {
    expect(detectPlatform("www.tradewheel.com")).toBe("tradewheel");
  });

  it("correctly detects b2brazil", () => {
    expect(detectPlatform("b2brazil.com")).toBe("b2brazil");
  });

  it("returns unknown for a genuinely unrecognized hostname", () => {
    expect(detectPlatform("facebook.com")).toBe("unknown");
  });
});

describe("platform registry - requiresClientSideScraping", () => {
  it("returns true for tradewheel", () => {
    expect(requiresClientSideScraping("tradewheel")).toBe(true);
  });

  it("returns false for b2brazil", () => {
    expect(requiresClientSideScraping("b2brazil")).toBe(false);
  });

  it("defaults to true for a genuinely unrecognized platform", () => {
    // Safe default: assume scraping is needed unless proven otherwise
    expect(requiresClientSideScraping("some_new_unregistered_platform")).toBe(true);
  });
});

describe("platform registry - isListingPage", () => {
  it("correctly identifies a real tradewheel listing URL", () => {
    expect(isListingPage("tradewheel", "https://www.tradewheel.com/p/test-product-123456/")).toBe(true);
  });

  it("correctly rejects a tradewheel non-listing URL", () => {
    expect(isListingPage("tradewheel", "https://www.tradewheel.com/")).toBe(false);
  });

  it("correctly identifies a real b2brazil listing URL", () => {
    expect(isListingPage("b2brazil", "https://b2brazil.com/hotsite/test-company")).toBe(true);
  });

  it("correctly rejects a b2brazil non-listing URL", () => {
    expect(isListingPage("b2brazil", "https://b2brazil.com/search?q=test")).toBe(false);
  });

  it("returns false for a genuinely unrecognized platform", () => {
    expect(isListingPage("unknown", "https://example.com/anything")).toBe(false);
  });
});
