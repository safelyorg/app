import { describe, it, expect, beforeAll } from "vitest";

// Which Made-in-China pages Safely scans (a product page or a
// supplier's own page), and the trimmed copy of a page it sends.

// scrapers/index.ts is a plain script (no exports): loading it puts its
// functions on window.__safelyScrapers. The path is passed as a string
// so the editor does not ask for exports.
const SCRAPERS = "../ts/scrapers/index";
const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);

let scrapers: any;

beforeAll(async () => {
  await load(SCRAPERS);
  scrapers = (window as any).__safelyScrapers;
});

describe("Made-in-China pages", () => {
  it("scans a product page on a supplier's shop or on the main site", () => {
    expect(
      scrapers.isMadeinchinaScanUrl(
        "https://yongjie-bearing.en.made-in-china.com/product/cEvrzSKdrhpB/China-Heavy-Duty-Cat-Excavator-Pins.html",
      ),
    ).toBe(true);
    expect(
      scrapers.isMadeinchinaScanUrl(
        "https://www.made-in-china.com/product/Mini-Laptop-PC-Ultra-Thin-Laptop-iUnRBMlVaLkP.html",
      ),
    ).toBe(true);
  });

  it("scans a supplier's own page", () => {
    expect(scrapers.isMadeinchinaScanUrl("https://sysmartlink.en.made-in-china.com/")).toBe(true);
    expect(scrapers.isMadeinchinaScanUrl("https://sysmartlink.en.made-in-china.com")).toBe(true);
  });

  it("does not scan list pages or other sites", () => {
    expect(scrapers.isMadeinchinaScanUrl("https://www.made-in-china.com/")).toBe(false);
    expect(
      scrapers.isMadeinchinaScanUrl("https://www.made-in-china.com/products-search/hot-china-products/Laptop.html"),
    ).toBe(false);
    expect(scrapers.isMadeinchinaScanUrl("https://sysmartlink.en.made-in-china.com/product-list-1.html")).toBe(false);
    expect(scrapers.isMadeinchinaScanUrl("https://made-in-china.com.fake.example/product/x.html")).toBe(false);
  });

  it("sends the page without scripts, styles or tracking, but keeps its data", () => {
    const doc = new DOMParser().parseFromString(
      `<html><head><link rel="canonical" href="https://yongjie-bearing.en.made-in-china.com"/>
        <link rel="stylesheet" href="a.css"><style>.x{}</style><script>track()</script></head>
        <body><h1>Changzhou Nanyi Bearing Co., Ltd.</h1>
        <script type="application/ld+json">{"@type":"Organization","name":"Changzhou Nanyi Bearing Co., Ltd."}</script>
        <iframe src="ads.html"></iframe><div id="safely-root">panel</div></body></html>`,
      "text/html",
    );
    const html: string = scrapers.madeinchinaTrimmed(doc);
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html).toContain('rel="canonical"');
    expect(html).toContain("application/ld+json");
    expect(html).toContain("Changzhou Nanyi Bearing Co., Ltd.");
    expect(html).not.toContain("track()");
    expect(html).not.toContain("stylesheet");
    expect(html).not.toContain("<style");
    expect(html).not.toContain("<iframe");
    expect(html).not.toContain("safely-root");
  });
});
