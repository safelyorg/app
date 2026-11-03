import { describe, it, expect, vi, beforeAll, afterAll } from "vitest";
import "../ts/scrapers/index";

// "Check this website": robots.txt rules, the shop's photos, and the
// pages read from the real site in the buyer's browser (Safely's own
// panel is never read as part of the shop).

const scrapers = (window as any).__safelyScrapers;

describe("robots.txt - wildcards and Allow", () => {
  const robots =
    "User-agent: *\nDisallow: /*?add-to-cart=\nDisallow: /wp-admin/\nAllow: /wp-admin/admin-ajax.php\nDisallow: /*.pdf$\n";

  it("reads the closed and the open rules for every robot", () => {
    expect(scrapers.robotsClosedPaths(robots)).toEqual(["/*?add-to-cart=", "/wp-admin/", "/*.pdf$"]);
    expect(scrapers.robotsOpenPaths(robots)).toEqual(["/wp-admin/admin-ajax.php"]);
  });

  it("matches * anywhere and $ at the end, including the ?query", () => {
    const closed = scrapers.robotsClosedPaths(robots);
    const open = scrapers.robotsOpenPaths(robots);
    const isClosed = (url: string) => scrapers.isClosedByRobots(url, closed, open);
    expect(isClosed("https://shop.com/product/1?add-to-cart=5")).toBe(true);
    expect(isClosed("https://shop.com/files/catalogue.pdf")).toBe(true);
    expect(isClosed("https://shop.com/files/catalogue.pdf.html")).toBe(false);
    expect(isClosed("https://shop.com/about-us/")).toBe(false);
  });

  it("lets the longer rule win, and Allow win a tie", () => {
    const closed = scrapers.robotsClosedPaths(robots);
    const open = scrapers.robotsOpenPaths(robots);
    expect(scrapers.isClosedByRobots("https://shop.com/wp-admin/edit.php", closed, open)).toBe(true);
    expect(scrapers.isClosedByRobots("https://shop.com/wp-admin/admin-ajax.php", closed, open)).toBe(false);
    expect(scrapers.isClosedByRobots("https://shop.com/a", ["/a"], ["/a"])).toBe(false);
  });

  // Real case (xtoinc.com, Nov 3): "Disallow:" rules for other robots
  // only must not close the site to Safely.
  it("ignores rules written for one named robot", () => {
    const closed = scrapers.robotsClosedPaths("User-agent: AhrefsBot\nDisallow: /\n\nUser-agent: *\nDisallow:\n");
    expect(closed).toEqual([]);
    expect(scrapers.isClosedByRobots("https://shop.com/contact-us/", closed, [])).toBe(false);
  });
});

describe("website photos", () => {
  // Real addresses (xtoinc.com and gloflux.com, Nov 3).
  it("keeps product photos and skips icons, logos and trackers", () => {
    const doc = new DOMParser().parseFromString(
      "<html><body>" +
        '<img src="https://www.xtoinc.com/wp-content/themes/xto-default/img/ico-exit.svg">' +
        '<img src="https://www.webtraxs.com/webtraxs.php?id=wt-28d89fd5&st=img">' +
        '<img src="/img/logo.png" alt="Shop">' +
        '<img src="/img/spacer.gif" width="1" height="1">' +
        '<img src="https://www.gloflux.com/wp-content/uploads/2026/04/banner_04_pc_0430_new.webp">' +
        '<img src="/products/battery.jpg">' +
        "</body></html>",
      "text/html",
    );
    const page = scrapers.websitePageFrom(doc, "https://www.gloflux.com/");
    expect(page.images).toEqual([
      "https://www.gloflux.com/wp-content/uploads/2026/04/banner_04_pc_0430_new.webp",
      "https://www.gloflux.com/products/battery.jpg",
    ]);
  });
});

describe("website pages - which ones", () => {
  it("also reads the FAQ page (payment is often explained there)", () => {
    // Real links (gloflux.com, Nov 3).
    const urls = scrapers.websitePagesToRead(
      {
        url: "https://www.gloflux.com/",
        title: "",
        text: "",
        images: [],
        links: [
          { href: "https://www.gloflux.com/about-us/", text: "About Us" },
          { href: "https://www.gloflux.com/contact/", text: "Contact" },
          { href: "https://www.gloflux.com/faqs/", text: "Resource" },
          { href: "https://www.gloflux.com/privacy-policy/", text: "Privacy Policy | GLOFLUX" },
        ],
      },
      false,
    );
    expect(urls).toEqual([
      "https://www.gloflux.com/about-us/",
      "https://www.gloflux.com/contact/",
      "https://www.gloflux.com/privacy-policy/",
      "https://www.gloflux.com/faqs/",
    ]);
  });
});

describe("collectWebsitePages - the pages sent to Safely", () => {
  // jsdom has no innerText; this stand-in is the text of what is shown.
  const realInnerText = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "innerText");
  beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "innerText", {
      configurable: true,
      get(this: HTMLElement) {
        return this.textContent || "";
      },
    });
  });
  afterAll(() => {
    if (realInnerText) Object.defineProperty(HTMLElement.prototype, "innerText", realInnerText);
    else delete (HTMLElement.prototype as any).innerText;
  });

  function site(): string[] {
    document.body.innerHTML =
      "<header>Acme Shop - sales@acme.test</header>" +
      "<script>var secret = 'not text';</script>" +
      '<main><a href="/about-us">About us</a> <a href="/terms">Terms</a> <a href="/faq">FAQ</a></main>' +
      '<div id="safely-root"><div>Safely EN Reading the shop\'s pages</div></div>';
    const fetched: string[] = [];
    (globalThis as any).fetch = vi.fn(async (url: string) => {
      fetched.push(url);
      if (url.endsWith("/robots.txt")) {
        return new Response("User-agent: *\nDisallow: /terms\n", { status: 200 });
      }
      return new Response("<html><head><title>Page</title></head><body>Page " + url + "</body></html>", {
        status: 200,
        headers: { "content-type": "text/html; charset=utf-8" },
      });
    });
    return fetched;
  }

  it("never reads Safely's own panel or the page's scripts as the shop's text", async () => {
    site();
    const pages = await scrapers.collectWebsitePages();
    expect(pages[0].text).toContain("Acme Shop - sales@acme.test");
    expect(pages[0].text).toContain("About us");
    expect(pages[0].text).not.toContain("Reading the shop's pages");
    expect(pages[0].text).not.toContain("secret");
  });

  it("reads the other pages of the same site, except the ones robots.txt closes", async () => {
    const fetched = site();
    const pages = await scrapers.collectWebsitePages();
    const origin = window.location.origin;
    expect(fetched).toContain(origin + "/robots.txt");
    expect(fetched).toContain(origin + "/about-us");
    expect(fetched).toContain(origin + "/faq");
    expect(fetched).not.toContain(origin + "/terms");
    expect(pages.map((p: any) => p.url)).toContain(origin + "/about-us");
  });
});
