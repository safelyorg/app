import { describe, it, expect, beforeAll, afterEach, vi } from "vitest";
import { runScript } from "./tab-helpers";

// Wholesale Central in the extension: which pages Safely scans (a
// product page or a supplier page, never a list page), and the copies
// of the pages sent to Safely (data scripts only).

let scrapers: any;

beforeAll(async () => {
  await runScript("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
  window.history.replaceState({}, "", "/");
});

describe("Wholesale Central pages", () => {
  const isScanUrl = (url: string) => scrapers.isWholesalecentralScanUrl(url);

  it("scans a product page", () => {
    expect(isScanUrl("https://www.wholesalecentral.com/products/33417104")).toBe(true);
  });

  it("scans a supplier page", () => {
    expect(isScanUrl("https://www.wholesalecentral.com/suppliers/luxiso0001")).toBe(true);
    expect(isScanUrl("https://www.wholesalecentral.com/suppliers/upsell0001?tab=about")).toBe(true);
  });

  it("does not scan list pages", () => {
    expect(isScanUrl("https://www.wholesalecentral.com/category/Electronics")).toBe(false);
    expect(isScanUrl("https://www.wholesalecentral.com/")).toBe(false);
    expect(isScanUrl("https://www.wholesalecentral.com/search?q=toys")).toBe(false);
    expect(isScanUrl("https://www.wholesalecentral.com/products/")).toBe(false);
    expect(isScanUrl("https://www.wholesalecentral.com/products/abc")).toBe(false);
  });
});

const DATA = 'self.__next_f.push([1,"b:{\\"x\\":1}\\n"])';

function pageWith(body: string): Document {
  return new DOMParser().parseFromString(
    "<html><head><script>trackBuyer()</script></head><body>" + body + "</body></html>",
    "text/html",
  );
}

describe("Wholesale Central page copies", () => {
  it("keeps only the data scripts", () => {
    const doc = pageWith("<p>Hello buyer@example.com</p><script>" + DATA + "</script>");
    const copy: string = scrapers.nextDataOnlyHtml(doc);
    expect(copy).toContain(DATA);
    expect(copy).not.toContain("trackBuyer");
    expect(copy).not.toContain("buyer@example.com");
  });

  it("gives nothing for a page without data", () => {
    expect(scrapers.nextDataOnlyHtml(pageWith("<p>No data</p>"))).toBeNull();
  });

  it("loads the supplier page from a product page, in this browser", async () => {
    window.history.replaceState({}, "", "/products/33248501");
    document.body.innerHTML = '<a href="/suppliers/iphoenix">iPhoenix</a>';
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      text: async () => "<html><body><script>" + DATA + "</script></body></html>",
    });
    (globalThis as any).fetch = fetchMock;

    const copy: string = await scrapers.fetchWholesalecentralSupplierPage();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain("/suppliers/iphoenix");
    expect(fetchMock.mock.calls[0][1]).toEqual({ credentials: "include" });
    expect(copy).toContain(DATA);
  });

  it("loads nothing more on a supplier page", async () => {
    window.history.replaceState({}, "", "/suppliers/iphoenix");
    const fetchMock = vi.fn();
    (globalThis as any).fetch = fetchMock;
    expect(await scrapers.fetchWholesalecentralSupplierPage()).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("gives nothing when the supplier page cannot be loaded", async () => {
    window.history.replaceState({}, "", "/products/1");
    document.body.innerHTML = '<a href="/suppliers/iphoenix">iPhoenix</a>';
    vi.spyOn(console, "error").mockImplementation(() => {});
    (globalThis as any).fetch = vi.fn().mockRejectedValue(new Error("offline"));
    expect(await scrapers.fetchWholesalecentralSupplierPage()).toBeNull();
  });
});
