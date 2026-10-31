import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeChrome } from "./setup-chrome";
import { installSharedGlobals, runScript } from "./tab-helpers";
import "../ts/scrapers/index";

// "Check this website": which of the shop's pages are read, robots.txt,
// and the button in the panel (shown only on an ordinary website, and
// nothing is read before it is clicked).

const scrapers = (window as any).__safelyScrapers;

describe("website pages read", () => {
  const page = (links: Array<[string, string]>) => ({
    url: "https://www.shop.com/products/phone",
    title: "Phone",
    text: "",
    links: links.map(([href, text]) => ({ href, text })),
  });

  it("reads the home page and one page of each kind, on the same site only", () => {
    const urls = scrapers.websitePagesToRead(
      page([
        ["https://www.shop.com/pages/about-us", "About us"],
        ["https://shop.com/pages/contato", "Fale conosco"],
        ["https://www.shop.com/policies/refund-policy", "Refunds"],
        ["https://www.shop.com/pages/faq", "Shipping and delivery"],
        ["https://other.com/terms", "Terms"],
        ["mailto:help@shop.com", "Email us"],
        ["https://www.shop.com/products/phone#reviews", "Reviews"],
      ]),
      false,
    );
    expect(urls).toEqual([
      "https://www.shop.com/",
      "https://www.shop.com/pages/about-us",
      "https://shop.com/pages/contato",
      "https://www.shop.com/pages/faq",
      "https://www.shop.com/policies/refund-policy",
    ]);
  });

  it("adds Shopify's fixed policy pages on a Shopify shop", () => {
    const urls = scrapers.websitePagesToRead(page([]), true);
    expect(urls).toContain("https://www.shop.com/policies/privacy-policy");
    expect(urls).toContain("https://www.shop.com/policies/shipping-policy");
    expect(urls.length).toBeLessThanOrEqual(11);
  });

  it("keeps the text and the links of a page, not its scripts", () => {
    const doc = new DOMParser().parseFromString(
      '<html><head><title>Contact</title></head><body><p>Write to <a href="mailto:a@shop.com">a@shop.com</a></p><script>var secret = 1;</script><a href="/refunds">Refunds</a></body></html>',
      "text/html",
    );
    const p = scrapers.websitePageFrom(doc, "https://shop.com/contact");
    expect(p.title).toBe("Contact");
    expect(p.text).toContain("Write to");
    expect(p.text).not.toContain("secret");
    expect(p.links.map((l: any) => l.href)).toEqual(["mailto:a@shop.com", "https://shop.com/refunds"]);
  });
});

describe("robots.txt", () => {
  it("follows the rules for every robot only", () => {
    const closed = scrapers.robotsClosedPaths(
      "User-agent: Googlebot\nDisallow: /private\n\nUser-agent: *\nDisallow: /checkout\nDisallow: /account # members\nAllow: /\n",
    );
    expect(closed).toEqual(["/checkout", "/account"]);
    expect(scrapers.isClosedByRobots("https://shop.com/checkout/1", closed)).toBe(true);
    expect(scrapers.isClosedByRobots("https://shop.com/pages/about", closed)).toBe(false);
  });

  it("reads nothing more when the whole site is closed", () => {
    const closed = scrapers.robotsClosedPaths("User-agent: *\nDisallow: /\n");
    expect(scrapers.isClosedByRobots("https://shop.com/pages/about", closed)).toBe(true);
  });
});

// ---- The button in the panel --------------------------------------

const watchers: MutationObserver[] = [];
const RealMutationObserver = globalThis.MutationObserver;
class TrackedMutationObserver extends RealMutationObserver {
  constructor(callback: MutationCallback) {
    super(callback);
    watchers.push(this);
  }
}
(globalThis as any).MutationObserver = TrackedMutationObserver;

async function loadPanel(websiteCheckable: boolean, marketplaceMessage: string | null) {
  document.body.innerHTML = "";
  vi.resetModules();
  installSharedGlobals();
  (fakeChrome.storage.local as any).get = vi.fn((_key: unknown, cb?: (r: any) => void) => {
    const result = { safely_session_token: "token" };
    if (cb) cb(result);
    return Promise.resolve(result);
  });
  const api = {
    SITE_BASE: "https://safely.sh",
    fetchAnalysis: vi.fn(),
    checkWebsite: vi.fn(),
    getScanUsage: vi.fn().mockResolvedValue(null),
  };
  (window as any).__safelyAPI = api;
  (window as any).__safelyScrapers = {
    isListingPage: vi.fn().mockReturnValue(false),
    isWebsiteCheckable: vi.fn().mockReturnValue(websiteCheckable),
    getUnsupportedMessage: vi.fn().mockReturnValue(marketplaceMessage),
  };
  (window as any).__safelyResetState = vi.fn();
  (globalThis as any).__safelyFormatUsageLine = () => "";
  (globalThis as any).fetch = vi.fn().mockResolvedValue({ arrayBuffer: async () => new ArrayBuffer(0) });
  await runScript("../ts/core/panel");
  (window as any).__safelyAddTab("risk", "Risk", "<p>risk tab</p>", "<svg></svg>", null);
  return api;
}

const button = () => document.getElementById("safely-check-website-btn") as HTMLElement;

describe("panel - Check this website", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    watchers.splice(0).forEach((w) => w.disconnect());
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("offers the button on an ordinary website and reads nothing before the click", async () => {
    const api = await loadPanel(true, null);
    expect(button().style.display).toBe("");
    expect(button().textContent).toBe("Check this website");
    expect(document.getElementById("safely-unsupported-title")!.textContent).toBe("Is this shop safe?");
    expect(api.checkWebsite).not.toHaveBeenCalled();
    expect(api.fetchAnalysis).not.toHaveBeenCalled();

    button().click();
    expect(api.checkWebsite).toHaveBeenCalledTimes(1);
    expect(document.getElementById("safely-checking")!.textContent).toContain("Reading the shop's pages");
  });

  it("Reload runs the website check again when that check did not finish", async () => {
    const api = await loadPanel(true, null);
    button().click();
    window.dispatchEvent(new CustomEvent("safely-analysis-finished", { detail: { error: "generic" } }));
    (document.getElementById("safely-retry-btn") as HTMLElement).click();
    expect(api.checkWebsite).toHaveBeenCalledTimes(2);
  });

  it("keeps a marketplace's own message and no button on its other pages", async () => {
    await loadPanel(false, "Open a product page on this marketplace.");
    expect(button().style.display).toBe("none");
    expect(document.getElementById("safely-unsupported-message")!.textContent).toBe(
      "Open a product page on this marketplace.",
    );
  });
});
