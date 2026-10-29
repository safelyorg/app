import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// A Made-in-China product page on the main site (www.made-in-china.com)
// cannot load the supplier's shop itself, so the background script
// loads it (only that kind of page).

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;
let isAllowed: any;

const SHOP = "https://szclover.en.made-in-china.com/";
const PRODUCT = "https://www.made-in-china.com/product/Office-Chair-ABC123.html";
const DATA = `<script type="application/ld+json">{"@type":"Organization","url":"https://szclover.en.made-in-china.com","name":"Shenzhen Clover"}</script>`;

beforeAll(async () => {
  (globalThis as any).chrome.runtime.id = "safely-extension";
  await load("../ts/scrapers/index");
  await load("../ts/background");
  scrapers = (window as any).__safelyScrapers;
  isAllowed = (globalThis as any).__safelyIsAllowedSupplierPage;
});

function onPage(href: string, body: string): void {
  const url = new URL(href);
  Object.defineProperty(window, "location", {
    value: { href, hostname: url.hostname, pathname: url.pathname, origin: url.origin },
    writable: true,
  });
  document.body.innerHTML = body;
}

describe("Made-in-China supplier page from a main-site product page", () => {
  it("is loaded by the background script", async () => {
    onPage(PRODUCT, DATA);
    const send = (globalThis as any).chrome.runtime.sendMessage;
    send.mockImplementation((message: any, reply: (r: unknown) => void) => {
      expect(message).toEqual({ type: "SAFELY_FETCH_SUPPLIER_PAGE", url: SHOP });
      reply({ html: "<html><body><div class='detail-address'>Guangdong, China</div></body></html>" });
    });
    expect(await scrapers.fetchMadeinchinaSupplierPage()).toContain("Guangdong, China");
    send.mockReset();
  });

  it("is null when the background script could not load it", async () => {
    onPage(PRODUCT, DATA);
    const send = (globalThis as any).chrome.runtime.sendMessage;
    send.mockImplementation((_m: unknown, reply: (r: unknown) => void) => reply({ html: null }));
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(await scrapers.fetchMadeinchinaSupplierPage()).toBe(null);
    send.mockReset();
  });

  it("the background script allows only Made-in-China shop pages", () => {
    expect(isAllowed(SHOP)).toBe(true);
    expect(isAllowed("https://szclover.en.made-in-china.com/product/x.html")).toBe(false);
    expect(isAllowed("https://www.made-in-china.com/")).toBe(false);
    expect(isAllowed("https://szclover.en.made-in-china.com.evil.example/")).toBe(false);
  });
});
