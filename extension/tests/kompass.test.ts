import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads Kompass pages in the browser: the product page is
// copied from the open tab, and the company page is loaded with the
// buyer's cookies (without them Kompass shows its robot check).

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const COMPANY = "https://www.kompass.com/c/ampco-metal-s-a/ch258855/";
const NAV = `<nav id="productNav"><a class="link itemLink" href="/c/ampco-metal-s-a/ch258855/" title="See the company">About us</a></nav>`;

beforeAll(async () => {
  await load("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

const page = (body: string) => new DOMParser().parseFromString(body, "text/html");

function onPage(path: string, body: string): void {
  Object.defineProperty(window, "location", {
    value: { href: "https://www.kompass.com" + path, hostname: "www.kompass.com", pathname: path },
    writable: true,
  });
  document.body.innerHTML = body;
}

describe("Kompass company page link", () => {
  it("is the 'See the company' link, made absolute", () => {
    expect(scrapers.kompassCompanyUrl(page(NAV))).toBe(COMPANY);
  });

  it("keeps other Kompass country sites", () => {
    const doc = page(
      `<nav id="productNav"><a href="https://fr.kompass.com/c/acme/fr123456/?x=1" title="See the company">x</a></nav>`,
    );
    expect(scrapers.kompassCompanyUrl(doc)).toBe("https://fr.kompass.com/c/acme/fr123456/");
  });

  it("is null when there is no link or it goes somewhere else", () => {
    expect(scrapers.kompassCompanyUrl(page("<p>nothing</p>"))).toBe(null);
    expect(
      scrapers.kompassCompanyUrl(
        page(`<nav id="productNav"><a href="https://evil.example/c/x/y1/" title="See the company">x</a></nav>`),
      ),
    ).toBe(null);
  });
});

describe("Kompass pages from the browser", () => {
  it("copies the product page from the tab, without scripts or Safely's own panel", () => {
    onPage(
      "/p/ampco-metal-s-a/ch258855/ampco-18/abc/",
      `<h1 itemprop="name" class="titleGeneral">AMPCO METAL S.A.</h1>${NAV}<script>tracking()</script><div id="safely-root">panel</div>`,
    );
    const html = scrapers.kompassPageHtml();
    expect(html).toContain("AMPCO METAL S.A.");
    expect(html).toContain('title="See the company"');
    expect(html).not.toContain("tracking()");
    expect(html).not.toContain("safely-root");
  });

  it("sends nothing when the tab itself shows the robot check", () => {
    onPage("/p/x/ch1/y/z/", `<p id="cmsg">Please enable JS and disable any ad blocker</p>`);
    expect(scrapers.kompassPageHtml()).toBe(null);
  });

  it("loads the company page with the buyer's cookies", async () => {
    onPage("/p/ampco-metal-s-a/ch258855/ampco-18/abc/", NAV);
    const fetchMock = vi.fn().mockResolvedValue(new Response("<html><body><div id='description'>Alloys</div></body></html>", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const html = await scrapers.fetchKompassCompanyPage();
    expect(html).toContain("Alloys");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.objectContaining({ credentials: "include" }));
    vi.unstubAllGlobals();
  });

  it("gives up on the robot check, so Safely's server loads the company page", async () => {
    onPage("/p/ampco-metal-s-a/ch258855/ampco-18/abc/", NAV);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("<p>Please enable JS and disable any ad blocker</p>", { status: 200 })),
    );
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(await scrapers.fetchKompassCompanyPage()).toBe(null);
    vi.unstubAllGlobals();
  });

  it("loads no company page on a company page", async () => {
    onPage("/c/ampco-metal-s-a/ch258855/", NAV);
    expect(await scrapers.fetchKompassCompanyPage()).toBe(null);
  });
});
