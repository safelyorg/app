import { describe, it, expect, beforeAll, vi } from "vitest";
// Sets up the fake chrome object the background script needs.
import "./setup-chrome";

// How Safely finds an Alibaba supplier's company page on a product page,
// and the background script that loads that page (and nothing else).

// Both files are plain scripts (no exports): loading them puts their
// functions on window / globalThis.
const SCRAPERS = "../ts/scrapers/index";
const BACKGROUND = "../ts/background";
const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);

const COMPANY = "https://zl9809.en.alibaba.com/company_profile.html";

let scrapers: any;
let isAllowed: any;
let handleFetch: any;

beforeAll(async () => {
  (globalThis as any).chrome.runtime.id = "safely-extension";
  await load(SCRAPERS);
  await load(BACKGROUND);
  scrapers = (window as any).__safelyScrapers;
  isAllowed = (globalThis as any).__safelyIsAllowedSupplierPage;
  handleFetch = (globalThis as any).__safelyHandleFetchSupplierPageMessage;
});

const page = (body: string) => new DOMParser().parseFromString(body, "text/html");

describe("Alibaba company page", () => {
  it("is found from the Company profile link", () => {
    const doc = page(
      `<a href="https://www.alibaba.com/help">Help</a>
       <a href="https://zl9809.en.alibaba.com/company_profile.html?spm=a2700.details.0.0">Company profile</a>`,
    );
    expect(scrapers.alibabaCompanyUrl(doc)).toBe(COMPANY);
  });

  it("is found from the shop address in the product data when there is no link", () => {
    const doc = page(
      `<script>window.detailData = {"globalData":{"seller":{"subDomain":"zl9809.en.alibaba.com"}}};</script>`,
    );
    expect(scrapers.alibabaCompanyUrl(doc)).toBe(COMPANY);
  });

  it("turns the mobile shop address into the normal one", () => {
    const doc = page(`<a href="//ZL9809.m.en.alibaba.com/company_profile.html">Company profile</a>`);
    expect(scrapers.alibabaCompanyUrl(doc)).toBe(COMPANY);
  });

  it("finds nothing on other sites or without a company", () => {
    expect(scrapers.alibabaCompanyUrl(page(`<a href="https://evil.example/x">Company profile</a>`))).toBe(
      null,
    );
    expect(
      scrapers.alibabaCompanyUrl(
        page(`<a href="https://zl9809.en.alibaba.com.evil.example/">Company profile</a>`),
      ),
    ).toBe(null);
    expect(scrapers.alibabaCompanyUrl(page(`<p>no company here</p>`))).toBe(null);
  });
});

describe("background script: Alibaba company pages", () => {
  it("allows only Alibaba company pages", () => {
    expect(isAllowed(COMPANY)).toBe(true);
    expect(isAllowed("https://zl9809.en.alibaba.com/")).toBe(false);
    expect(isAllowed("https://www.alibaba.com/product-detail/x_1.html")).toBe(false);
    expect(isAllowed("https://zl9809.en.alibaba.com.evil.example/company_profile.html")).toBe(false);
    expect(isAllowed("http://zl9809.en.alibaba.com/company_profile.html")).toBe(false);
    expect(isAllowed(COMPANY + "?a=b")).toBe(false);
  });

  it("loads an allowed company page without the buyer's cookies", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response("<html><body>Boyue</body></html>", {
        status: 200,
        headers: { "content-type": "text/html;charset=UTF-8" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const reply = await new Promise<any>((resolve) => {
      handleFetch({ type: "SAFELY_FETCH_SUPPLIER_PAGE", url: COMPANY }, { id: "safely-extension" }, resolve);
    });
    expect(reply.html).toContain("Boyue");
    expect(fetchMock).toHaveBeenCalledWith(COMPANY, expect.objectContaining({ credentials: "omit" }));
    vi.unstubAllGlobals();
  });
});
