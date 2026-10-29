import { describe, it, expect, beforeAll, vi } from "vitest";
// Sets up the fake chrome object the background script needs.
import "./setup-chrome";

// Which Global Sources pages Safely scans (product pages only), how it
// finds the supplier's page, and the background script that loads that
// page (and nothing else).

// Both files are plain scripts (no exports): loading them puts their
// functions on window / globalThis. The paths are passed as strings so
// the editor does not ask for exports.
const SCRAPERS = "../ts/scrapers/index";
const BACKGROUND = "../ts/background";
const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);

const SUPPLIER = "https://boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm";

let scrapers: any;
let handleFetch: any;
let isAllowed: any;
let handleApi: any;
let isApiUrl: any;

beforeAll(async () => {
  (globalThis as any).chrome.runtime.id = "safely-extension";
  await load(SCRAPERS);
  await load(BACKGROUND);
  scrapers = (window as any).__safelyScrapers;
  handleFetch = (globalThis as any).__safelyHandleFetchSupplierPageMessage;
  isAllowed = (globalThis as any).__safelyIsAllowedSupplierPage;
  handleApi = (globalThis as any).__safelyHandleApiRequestMessage;
  isApiUrl = (globalThis as any).__safelyIsSafelyApiUrl;
});

describe("Global Sources pages", () => {
  it("scans a product page", () => {
    expect(
      scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com/TWS-earbud/tws-earbud-1231057850p.htm"),
    ).toBe(true);
    expect(
      scrapers.isGlobalsourcesScanUrl(
        "https://www.globalsources.com/TWS-earbud/tws-earbud-tws-headset-earbuds-wireless-earphone-1214783541p.htm?ver=1",
      ),
    ).toBe(true);
  });

  it("scans a trade-show product page", () => {
    expect(
      scrapers.isGlobalsourcesScanUrl(
        "https://www.globalsources.com/products/HK/angle-rock-0-40-degrees-and-2-heights-adjustable_1213293180/",
      ),
    ).toBe(true);
    expect(
      scrapers.isGlobalsourcesScanUrl(
        "https://www.globalsources.com/products/hk/angle-rock-0-40-degrees-and-2-heights-adjustable_1213293180",
      ),
    ).toBe(true);
  });

  it("does not scan supplier, exhibitor, list pages or other sites", () => {
    expect(scrapers.isGlobalsourcesScanUrl(SUPPLIER)).toBe(false);
    expect(scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com/")).toBe(false);
    expect(
      scrapers.isGlobalsourcesScanUrl(
        "https://www.globalsources.com/exhibitors/HK/anhui-dujia-office-supplies-manufacturing-co-ltd_2008801135123/",
      ),
    ).toBe(false);
    expect(scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com/products/HK/")).toBe(false);
    expect(
      scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com/products/HK/earbuds-1231057850p.htm"),
    ).toBe(false);
    expect(scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com/category/tws-earbuds_39376/")).toBe(
      false,
    );
    expect(
      scrapers.isGlobalsourcesScanUrl("https://www.globalsources.com.fake.example/TWS-earbud/x-1231057850p.htm"),
    ).toBe(false);
  });

  it("finds the supplier's page in the supplier box", () => {
    const doc = new DOMParser().parseFromString(
      `<a href="//www.globalsources.com/help.htm">Help</a>
       <div class="company-info"><div class="supplier-name">
         <a href="//boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm?from=pdt">Huizhou Boxin</a></div></div>`,
      "text/html",
    );
    expect(scrapers.globalsourcesSupplierUrl(doc)).toBe(SUPPLIER);
  });

  it("finds no supplier page when there is no link to one", () => {
    const doc = new DOMParser().parseFromString(
      `<a href="https://evil.example/homepage_1.htm">x</a>`,
      "text/html",
    );
    expect(scrapers.globalsourcesSupplierUrl(doc)).toBe(null);
  });
});

describe("Global Sources page copy", () => {
  it("keeps the supplier's contact data block but no other script", () => {
    const raw = `<html><head><script type="application/ld+json">{"@type":"Product"}</script></head><body>
      <h1>Earbuds</h1>
      <script>window.__NUXT__=(function(a){return {track:1,contactUsInfo:{companyName:a,address:"Rm 701, Huizhou",othHomePageAddress:"https:\\u002F\\u002Fboxinheadset.com\\u002F",registerNum:"91441303594089865C"},token:"secret"}}(""))</script>
    </body></html>`;
    const copy = scrapers.globalsourcesTrimmed(raw, new DOMParser().parseFromString(raw, "text/html"));
    expect(copy).toContain('contactUsInfo:{companyName:a,address:"Rm 701, Huizhou"');
    expect(copy).toContain("91441303594089865C");
    expect(copy).toContain('{"@type":"Product"}');
    expect(copy).not.toContain("__NUXT__");
    expect(copy).not.toContain("secret");
  });

  it("is the plain trimmed page when there is no contact data", () => {
    const raw = "<html><body><h1>x</h1><script>track()</script></body></html>";
    const copy = scrapers.globalsourcesTrimmed(raw, new DOMParser().parseFromString(raw, "text/html"));
    expect(copy).not.toContain("x-safely-data");
    expect(copy).not.toContain("track()");
  });
});

describe("Global Sources trade-show pages", () => {
  // Nuxt's page data: index 0 wraps 1; every field is the number of
  // another entry.
  const nuxt = JSON.stringify([
    ["ShallowReactive", 1],
    { data: 2, state: 9 },
    ["ShallowReactive", 3],
    { exhibitorProductDetail: 4 },
    { data: 5 },
    { supplierInfo: 6 },
    { supplierId: 7, companyName: 8 },
    2008801135123,
    "Anhui Dujia Office Supplies Manufacturing Co.,Ltd",
    { userEmail: 10 },
    "buyer@example.com",
  ]);
  const raw =
    '<html><body><h1>Angle rock</h1><script>track()</script>' +
    '<script type="application/json" id="__NUXT_DATA__">' + nuxt + "</script></body></html>";
  const PRODUCT = "https://www.globalsources.com/products/HK/angle-rock_1213293180/";

  it("finds the supplier's exhibitor page", () => {
    const doc = new DOMParser().parseFromString(raw, "text/html");
    expect(scrapers.globalsourcesExhibitorUrl(doc, PRODUCT)).toBe(
      "https://www.globalsources.com/exhibitors/HK/anhui-dujia-office-supplies-manufacturing-co-ltd_2008801135123/",
    );
  });

  it("keeps only the product and supplier data, not the visitor's own data", () => {
    const copy = scrapers.globalsourcesTrimmed(raw, new DOMParser().parseFromString(raw, "text/html"));
    expect(copy).toContain('id="__NUXT_DATA__"');
    expect(copy).toContain("Anhui Dujia Office Supplies Manufacturing Co.,Ltd");
    expect(copy).toContain("2008801135123");
    expect(copy).not.toContain("buyer@example.com");
    expect(copy).not.toContain("track()");
    // The copy can be read back the same way.
    const again = new DOMParser().parseFromString(copy, "text/html");
    expect(scrapers.globalsourcesExhibitorUrl(again, PRODUCT)).toContain("_2008801135123/");
  });
});

describe("background script: loading a supplier page", () => {
  it("allows only Global Sources supplier pages", () => {
    expect(isAllowed(SUPPLIER)).toBe(true);
    expect(isAllowed("https://www.globalsources.com/TWS-earbud/x-1231057850p.htm")).toBe(false);
    expect(isAllowed("https://evil.example/homepage_1.htm")).toBe(false);
    expect(isAllowed("https://x.manufacturer.globalsources.com.evil.example/homepage_1.htm")).toBe(false);
    expect(isAllowed("http://boxinltd.manufacturer.globalsources.com/homepage_1.htm")).toBe(false);
    expect(isAllowed(SUPPLIER + "?a=b")).toBe(false);
  });

  it("loads an allowed page and sends its HTML back", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response("<html><body>Boxin</body></html>", {
        status: 200,
        headers: { "content-type": "text/html; charset=utf-8" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const reply = await new Promise<any>((resolve) => {
      const keepOpen = handleFetch(
        { type: "SAFELY_FETCH_SUPPLIER_PAGE", url: SUPPLIER },
        { id: "safely-extension" },
        resolve,
      );
      expect(keepOpen).toBe(true);
    });
    expect(reply.html).toContain("Boxin");
    expect(fetchMock).toHaveBeenCalledWith(SUPPLIER, expect.objectContaining({ credentials: "omit" }));
    vi.unstubAllGlobals();
  });

  it("refuses other addresses and other senders without loading anything", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const replies: any[] = [];
    handleFetch(
      { type: "SAFELY_FETCH_SUPPLIER_PAGE", url: "https://evil.example/homepage_1.htm" },
      { id: "safely-extension" },
      (r: any) => replies.push(r),
    );
    handleFetch(
      { type: "SAFELY_FETCH_SUPPLIER_PAGE", url: SUPPLIER },
      { id: "another-extension" },
      (r: any) => replies.push(r),
    );
    expect(replies).toEqual([{ html: null }, { html: null }]);
    expect(fetchMock).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("leaves other messages to the other listener", () => {
    const reply = vi.fn();
    expect(handleFetch({ type: "SAFELY_SESSION_UPDATE", token: "t" }, { id: "safely-extension" }, reply)).toBe(
      false,
    );
    expect(reply).not.toHaveBeenCalled();
  });
});

describe("background script: calls to Safely's server", () => {
  it("allows only Safely's own API", () => {
    expect(isApiUrl("https://safely.sh/api/v1/analyze")).toBe(true);
    expect(isApiUrl("http://localhost:3000/api/v1/analyze")).toBe(true);
    expect(isApiUrl("https://safely.sh.evil.example/api/v1/analyze")).toBe(false);
    expect(isApiUrl("https://evil.example/api/v1/analyze")).toBe(false);
    expect(isApiUrl("https://safely.sh/blog")).toBe(false);
  });

  it("makes the call and sends back the status and text", async () => {
    (globalThis as any).chrome.runtime.getPlatformInfo = vi.fn();
    const fetchMock = vi.fn().mockResolvedValue(new Response('{"risk_score":40}', { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const reply = await new Promise<any>((resolve) => {
      const keepOpen = handleApi(
        {
          type: "SAFELY_API_REQUEST",
          url: "http://localhost:3000/api/v1/analyze",
          method: "POST",
          headers: { Authorization: "Bearer t" },
          body: "{}",
        },
        { id: "safely-extension" },
        resolve,
      );
      expect(keepOpen).toBe(true);
    });
    expect(reply).toEqual({ status: 200, text: '{"risk_score":40}' });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:3000/api/v1/analyze",
      expect.objectContaining({ method: "POST", body: "{}" }),
    );
    vi.unstubAllGlobals();
  });

  it("reports a network failure instead of throwing", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const reply = await new Promise<any>((resolve) =>
      handleApi(
        { type: "SAFELY_API_REQUEST", url: "https://safely.sh/api/v1/report", method: "GET", headers: {}, body: null },
        { id: "safely-extension" },
        resolve,
      ),
    );
    expect(reply.networkError).toContain("Failed to fetch");
    vi.unstubAllGlobals();
  });

  it("refuses other addresses without calling them", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const reply = vi.fn();
    handleApi(
      { type: "SAFELY_API_REQUEST", url: "https://evil.example/x", method: "GET", headers: {}, body: null },
      { id: "safely-extension" },
      reply,
    );
    expect(reply).toHaveBeenCalledWith({ networkError: "not allowed" });
    expect(fetchMock).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("content scripts get a normal response back", async () => {
    const send = (globalThis as any).chrome.runtime.sendMessage;
    send.mockImplementation((_m: unknown, cb: (r: unknown) => void) => cb({ status: 402, text: '{"error":"x"}' }));
    const response = await (window as any).__safelyFetch("https://safely.sh/api/v1/analyze", {
      method: "POST",
      body: "{}",
    });
    expect(response.status).toBe(402);
    expect(response.ok).toBe(false);
    expect(await response.text()).toBe('{"error":"x"}');
    send.mockReset();
  });
});
