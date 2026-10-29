import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads B2Brazil pages in the browser: the product page is
// copied from the open tab, and the company page is loaded from the
// same site.

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const PRODUCT = "https://b2brazil.com/hotsite/nutrircorretorade/soy";
const COMPANY = "https://b2brazil.com/hotsite/nutrircorretorade";

beforeAll(async () => {
  await load("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

function onPage(href: string, body: string): void {
  const url = new URL(href);
  Object.defineProperty(window, "location", {
    value: { href, hostname: url.hostname, pathname: url.pathname },
    writable: true,
  });
  document.body.innerHTML = body;
}

describe("B2Brazil company page address", () => {
  it("comes from the product page address", () => {
    expect(scrapers.b2brazilCompanyUrl(PRODUCT)).toBe(COMPANY);
    expect(scrapers.b2brazilCompanyUrl(PRODUCT + "?x=1")).toBe(COMPANY);
  });

  it("is null on pages that are not product pages", () => {
    expect(scrapers.b2brazilCompanyUrl(COMPANY)).toBe(null);
    expect(scrapers.b2brazilCompanyUrl("https://b2brazil.com/plans")).toBe(null);
  });
});

describe("B2Brazil pages from the browser", () => {
  it("copies the product page from the tab, without scripts or Safely's panel", () => {
    onPage(PRODUCT, `<h2 class="section-product-title">Soy</h2><script>track()</script><div id="safely-root">x</div>`);
    const html = scrapers.b2brazilPageHtml();
    expect(html).toContain("Soy");
    expect(html).not.toContain("track()");
    expect(html).not.toContain("safely-root");
  });

  it("loads the company page from the same site", async () => {
    onPage(PRODUCT, "");
    const fetchMock = vi.fn().mockResolvedValue(new Response("<html><body>Nutrir trades grain.</body></html>", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    expect(await scrapers.fetchB2brazilCompanyPage()).toContain("Nutrir trades grain.");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.anything());
    vi.unstubAllGlobals();
  });

  it("gives null when the company page does not load, so the server loads it", async () => {
    onPage(PRODUCT, "");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("", { status: 503 })));
    expect(await scrapers.fetchB2brazilCompanyPage()).toBe(null);
    vi.unstubAllGlobals();
  });
});
