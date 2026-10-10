import { describe, it, expect, vi, beforeEach } from "vitest";
import "../ts/scrapers/index";

const scrapers = (window as any).__safelyScrapers;

describe("normalize", () => {
  it("converts common lookalike characters to their real equivalent", () => {
    expect(scrapers.normalize("0lx.com")).toBe("olx.com");
    expect(scrapers.normalize("faceb00k.com")).toBe("facebook.com");
  });
});

describe("editDistance", () => {
  it("returns 0 for identical strings", () => {
    expect(scrapers.editDistance("olx.com.pk", "olx.com.pk")).toBe(0);
  });
  it("returns the correct distance for a single character change", () => {
    expect(scrapers.editDistance("olx.com.pk", "0lx.com.pk")).toBe(1);
  });
  it("returns the correct distance for genuinely different strings", () => {
    expect(scrapers.editDistance("cat", "dog")).toBe(3);
  });
});

describe("checkDomain", () => {
  beforeEach(async () => {
    // PROTECTED_DOMAINS is now populated via a real, live fetch to
    // the backend - for these tests, we bypass that by directly
    // seeding the cache loadProtectedDomains() reads from, so
    // checkDomain() has real, known data to compare against.
    (globalThis as any).chrome = {
      storage: {
        local: {
          get: vi.fn().mockResolvedValue({
            safely_protected_domains_cache: [
              { name: "OLX Pakistan", domain: "olx.com.pk" },
            ],
            safely_protected_domains_cached_at: Date.now(),
          }),
          set: vi.fn(),
        },
      },
    };
    await scrapers.loadProtectedDomains();
  });

  it("returns null for a domain with no relation to any protected site", () => {
    Object.defineProperty(window, "location", {
      value: { hostname: "some-genuinely-unrelated-site.com" },
      writable: true,
    });
    expect(scrapers.checkDomain()).toBeNull();
  });

  it("returns legitimate for the real, exact protected domain", () => {
    Object.defineProperty(window, "location", {
      value: { hostname: "olx.com.pk" },
      writable: true,
    });
    const result = scrapers.checkDomain();
    expect(result).not.toBeNull();
    expect(result.status).toBe("legitimate");
  });

  it("returns suspicious for a homoglyph lookalike domain", () => {
    Object.defineProperty(window, "location", {
      value: { hostname: "0lx.com.pk" },
      writable: true,
    });
    const result = scrapers.checkDomain();
    expect(result).not.toBeNull();
    expect(result.status).toBe("suspicious");
    expect(result.reason).toBe("lookalike-characters");
  });
});

describe("isGenuineDomain", () => {
  it("returns true for an exact match", () => {
    expect(scrapers.isGenuineDomain("olx.com.pk", "olx.com.pk")).toBe(true);
  });
  it("returns true for a genuine subdomain", () => {
    expect(scrapers.isGenuineDomain("m.olx.com.pk", "olx.com.pk")).toBe(true);
  });
  it("returns false for an unrelated domain", () => {
    expect(scrapers.isGenuineDomain("notolx.com", "olx.com.pk")).toBe(false);
  });
});

describe("highlightDiff", () => {
  it("marks the exact differing character on both sides of a substitution", () => {
    const result = scrapers.highlightDiff("0lx.com.pk", "olx.com.pk");
    expect(result.currentHtml).toContain("<mark");
    expect(result.realHtml).toContain("<mark");
  });
  it("marks nothing when both strings are identical", () => {
    const result = scrapers.highlightDiff("olx.com.pk", "olx.com.pk");
    expect(result.currentHtml).not.toContain("<mark");
    expect(result.realHtml).not.toContain("<mark");
  });
});

describe("detectPlatform", () => {
  it("returns 'olx' for a real olx.com.pk URL", () => {
    Object.defineProperty(window, "location", {
      value: {
        href: "https://www.olx.com.pk/item/some-listing-iid-123",
        hostname: "www.olx.com.pk",
      },
      writable: true,
    });
    expect(scrapers.detectPlatform()).toBe("olx");
  });

  it("returns 'unknown' for any other URL", () => {
    Object.defineProperty(window, "location", {
      value: {
        href: "https://www.facebook.com/marketplace/item/123",
        hostname: "www.facebook.com",
      },
      writable: true,
    });
    expect(scrapers.detectPlatform()).toBe("unknown");
  });
});

describe("isListingPage", () => {
  it("returns true for a real OLX listing URL containing iid-", () => {
    Object.defineProperty(window, "location", {
      value: {
        href: "https://www.olx.com.pk/item/some-listing-iid-123",
        hostname: "www.olx.com.pk",
      },
      writable: true,
    });
    expect(scrapers.isListingPage()).toBe(true);
  });

  it("returns false for an OLX URL that is not a specific listing", () => {
    Object.defineProperty(window, "location", {
      value: {
        href: "https://www.olx.com.pk/",
        hostname: "www.olx.com.pk",
      },
      writable: true,
    });
    expect(scrapers.isListingPage()).toBe(false);
  });

  it("returns false for a non-OLX URL", () => {
    Object.defineProperty(window, "location", {
      value: {
        href: "https://www.facebook.com/",
        hostname: "www.facebook.com",
      },
      writable: true,
    });
    expect(scrapers.isListingPage()).toBe(false);
  });
});

describe("B2Brazil pages", () => {
  function onPage(href: string): void {
    Object.defineProperty(window, "location", {
      value: { href, hostname: new URL(href).hostname },
      writable: true,
    });
  }

  it("scans a product listing", () => {
    onPage("https://b2brazil.com/hotsite/daakiiya/bethel-nut");
    expect(scrapers.isListingPage()).toBe(true);
    onPage("https://b2brazil.com.br/hotsite/daakiiya/noz-de-betel?ref=x#top");
    expect(scrapers.isListingPage()).toBe(true);
    onPage("https://b2brazil.com/hotsite/daakiiya/bethel-nut/");
    expect(scrapers.isListingPage()).toBe(true);
  });

  it("does not scan the company page or its product list", () => {
    for (const href of [
      "https://b2brazil.com/hotsite/daakiiya",
      "https://b2brazil.com/hotsite/daakiiya/",
      "https://b2brazil.com/hotsite/daakiiya/products",
      "https://b2brazil.com.br/hotsite/daakiiya/produtos",
      "https://es.b2brazil.com/hotsite/daakiiya/productos",
    ]) {
      onPage(href);
      expect(scrapers.isListingPage(), href).toBe(false);
    }
  });

  it("does not scan search, plans, blog or the home page", () => {
    for (const href of [
      "https://b2brazil.com/",
      "https://b2brazil.com/search/products?s=sugar",
      "https://b2brazil.com/plans",
      "https://b2brazil.com/blog",
      "https://b2brazil.com/leadscentral",
    ]) {
      onPage(href);
      expect(scrapers.isListingPage(), href).toBe(false);
    }
  });

  it("shows the normal message on a page it does not scan", () => {
    onPage("https://b2brazil.com/hotsite/daakiiya");
    expect(scrapers.detectPlatform()).toBe("b2brazil");
    expect(scrapers.getUnsupportedMessage()).toBeNull();
  });
});
