import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads TradeWheel pages in the browser: the product page is
// copied from the open tab, and the company page and its About Us page
// are loaded from the same site.

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const PRODUCT = "https://www.tradewheel.com/p/polo-shirts-2525134/";
const COMPANY = "https://www.tradewheel.com/co/embroiderylinksports/";
const BOX = `<div class="comp-info"><a href="${COMPANY}" title="Embroidery Link Sports"><h2>Embroidery Link Sports</h2></a></div>`;

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

describe("TradeWheel company page link", () => {
  it("is the company link in the company box", () => {
    expect(scrapers.tradewheelCompanyUrl(page(BOX))).toBe(COMPANY);
  });

  it("makes a relative link absolute and adds the /", () => {
    const doc = page(`<div class="comp-info"><a href="/co/abc-trading">ABC</a></div>`);
    expect(scrapers.tradewheelCompanyUrl(doc)).toBe("https://www.tradewheel.com/co/abc-trading/");
  });

  it("is null for other sites or no link", () => {
    expect(scrapers.tradewheelCompanyUrl(page("<p>none</p>"))).toBe(null);
    expect(
      scrapers.tradewheelCompanyUrl(page(`<div class="comp-info"><a href="https://evil.example/co/x/">x</a></div>`)),
    ).toBe(null);
  });
});

describe("TradeWheel pages from the browser", () => {
  it("copies the product page from the tab, without scripts or Safely's panel", () => {
    onPage(PRODUCT, `<h1 class="pd-heading">Polo Shirts</h1>${BOX}<script>track()</script><div id="safely-root">x</div>`);
    const html = scrapers.tradewheelPageHtml();
    expect(html).toContain("Polo Shirts");
    expect(html).not.toContain("track()");
    expect(html).not.toContain("safely-root");
  });

  it("loads the company page and its About Us page from the same site", async () => {
    onPage(PRODUCT, BOX);
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve(
        new Response(
          url.endsWith("about-us/")
            ? "<html><body>Website www.embroiderylinksports.com</body></html>"
            : "<html><body>Established Year 1998</body></html>",
          { status: 200 },
        ),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    const pages = await scrapers.fetchTradewheelCompanyPages();
    expect(pages.company).toContain("1998");
    expect(pages.extended).toContain("www.embroiderylinksports.com");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.anything());
    expect(fetchMock).toHaveBeenCalledWith(COMPANY + "about-us/", expect.anything());
    vi.unstubAllGlobals();
  });

  it("sends the company page even when About Us fails", async () => {
    onPage(PRODUCT, BOX);
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve(
        url.endsWith("about-us/")
          ? new Response("not found", { status: 404 })
          : new Response("<html><body>Established Year 1998</body></html>", { status: 200 }),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    const pages = await scrapers.fetchTradewheelCompanyPages();
    expect(pages.company).toContain("1998");
    expect(pages.extended).toBe(null);
    vi.unstubAllGlobals();
  });

  it("loads nothing on a company page", async () => {
    onPage(COMPANY, BOX);
    expect(await scrapers.fetchTradewheelCompanyPages()).toEqual({ company: null, extended: null });
  });
});
