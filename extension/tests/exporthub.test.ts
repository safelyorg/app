import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads ExportHub pages in the browser: the product page is
// copied from the open tab, and the company page and its second page
// ("profile.html") are loaded from the same site.

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const COMPANY = "https://www.exporthub.com/nanpixinchengweiyehardwareproducts/";
const SIDEBAR = `<div class="product-del_sidebar__comp-nm"><a href="${COMPANY}" title="Nanpi"><div class="product-del_sidebar__comp-ttl">Nanpi Xinchengweiye</div></a></div>`;

beforeAll(async () => {
  await load("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

const page = (body: string) => new DOMParser().parseFromString(body, "text/html");

function onPage(path: string, body: string): void {
  Object.defineProperty(window, "location", {
    value: { href: "https://www.exporthub.com" + path, hostname: "www.exporthub.com", pathname: path },
    writable: true,
  });
  document.body.innerHTML = body;
}

describe("ExportHub company page link", () => {
  it("is the company link in the sidebar", () => {
    expect(scrapers.exporthubCompanyUrl(page(SIDEBAR))).toBe(COMPANY);
  });

  it("adds the missing / and drops extras", () => {
    const doc = page(`<div class="product-del_sidebar__comp-nm"><a href="/hym-textile-11125866?x=1">x</a></div>`);
    expect(scrapers.exporthubCompanyUrl(doc)).toBe("https://www.exporthub.com/hym-textile-11125866/");
  });

  it("is null for other sites or no link", () => {
    expect(scrapers.exporthubCompanyUrl(page("<p>none</p>"))).toBe(null);
    expect(
      scrapers.exporthubCompanyUrl(page(`<div class="product-del_sidebar__comp-nm"><a href="https://evil.example/x/">x</a></div>`)),
    ).toBe(null);
  });
});

describe("ExportHub pages from the browser", () => {
  it("copies the product page from the tab, without scripts or Safely's panel", () => {
    onPage("/product/metal-electronics-24647395.html", `<h1 class="prod-dtl_ttl">Metal parts</h1>${SIDEBAR}<script>track()</script><div id="safely-root">x</div>`);
    const html = scrapers.exporthubPageHtml();
    expect(html).toContain("Metal parts");
    expect(html).not.toContain("track()");
    expect(html).not.toContain("safely-root");
  });

  it("loads both company pages from the same site", async () => {
    onPage("/product/metal-electronics-24647395.html", SIDEBAR);
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(new Response(`<html><body>${url.endsWith("profile.html") ? "second" : "first"}</body></html>`, { status: 200 })),
    );
    vi.stubGlobal("fetch", fetchMock);
    const pages = await scrapers.fetchExporthubCompanyPages();
    expect(pages.company).toContain("first");
    expect(pages.extended).toContain("second");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.anything());
    expect(fetchMock).toHaveBeenCalledWith(COMPANY + "profile.html", expect.anything());
    vi.unstubAllGlobals();
  });

  it("gives null for a page that did not load, so the server loads it", async () => {
    onPage("/product/metal-electronics-24647395.html", SIDEBAR);
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(new Response("x", { status: url.endsWith("profile.html") ? 500 : 200 })),
      ),
    );
    const pages = await scrapers.fetchExporthubCompanyPages();
    expect(pages.company).not.toBe(null);
    expect(pages.extended).toBe(null);
    vi.unstubAllGlobals();
  });

  it("loads nothing on a company page", async () => {
    onPage("/nanpixinchengweiyehardwareproducts/", SIDEBAR);
    expect(await scrapers.fetchExporthubCompanyPages()).toEqual({ company: null, extended: null });
  });
});
