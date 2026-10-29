import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads B2BMap pages in the browser: the product page is
// copied from the open tab, and the company page is loaded from the
// same site.

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const PRODUCT = "https://b2bmap.com/products/persy-four-soundproof-meeting-pod-germany";
const COMPANY = "https://b2bmap.com/persy-booths";
const NAME = `<h4 class="text-18 text-lg-22"><a href="${COMPANY}" class="d-block text-strong">Persy Booths</a></h4>`;

beforeAll(async () => {
  await load("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

const page = (body: string) => new DOMParser().parseFromString(body, "text/html");

function onPage(href: string, body: string): void {
  const url = new URL(href);
  Object.defineProperty(window, "location", {
    value: { href, hostname: url.hostname, pathname: url.pathname },
    writable: true,
  });
  document.body.innerHTML = body;
}

describe("B2BMap company page link", () => {
  it("is the supplier name link", () => {
    expect(scrapers.b2bmapCompanyUrl(page(NAME))).toBe(COMPANY);
  });

  it("keeps only the company part of the address", () => {
    const doc = page(`<h4 class="text-18 text-lg-22"><a href="/Persy-Booths/contact-info">x</a></h4>`);
    expect(scrapers.b2bmapCompanyUrl(doc)).toBe(COMPANY);
  });

  it("is null for b2bmap's own sections, other sites or no link", () => {
    expect(scrapers.b2bmapCompanyUrl(page("<p>none</p>"))).toBe(null);
    expect(
      scrapers.b2bmapCompanyUrl(page(`<h4 class="text-18 text-lg-22"><a href="https://b2bmap.com/products/x">x</a></h4>`)),
    ).toBe(null);
    expect(
      scrapers.b2bmapCompanyUrl(page(`<h4 class="text-18 text-lg-22"><a href="https://evil.example/persy">x</a></h4>`)),
    ).toBe(null);
  });
});

describe("B2BMap pages from the browser", () => {
  it("copies the product page from the tab, without scripts or Safely's panel", () => {
    onPage(PRODUCT, `<h1 class="text-18 text-md-26 text-strong">Persy Four</h1>${NAME}<script>track()</script><div id="safely-root">x</div>`);
    const html = scrapers.b2bmapPageHtml();
    expect(html).toContain("Persy Four");
    expect(html).not.toContain("track()");
    expect(html).not.toContain("safely-root");
  });

  it("loads the company page from the same site", async () => {
    onPage(PRODUCT, NAME);
    const fetchMock = vi.fn().mockResolvedValue(new Response("<html><body>Founded in 2019</body></html>", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    expect(await scrapers.fetchB2bmapCompanyPage()).toContain("2019");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.anything());
    vi.unstubAllGlobals();
  });

  it("loads nothing on a company page", async () => {
    onPage(COMPANY, NAME);
    expect(await scrapers.fetchB2bmapCompanyPage()).toBe(null);
  });
});
