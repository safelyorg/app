import { describe, it, expect, beforeAll, vi } from "vitest";
import "./setup-chrome";

// How Safely reads a ThomasNet profile page in the browser. It is also
// the company page, so it is the only page sent, with its data script.

const load = (path: string): Promise<unknown> => import(/* @vite-ignore */ path);
let scrapers: any;

const PATH = "/company/applied-rapid-technologies-1095055/profile";
const DATA = `<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"canonicalUrl":"https://www.thomasnet.com/company/applied-rapid-technologies-1095055/profile","data":{"name":"Applied Rapid Technologies"}}}}</script>`;

beforeAll(async () => {
  await load("../ts/scrapers/index");
  scrapers = (window as any).__safelyScrapers;
});

function onPage(path: string, head: string, body: string): void {
  Object.defineProperty(window, "location", {
    value: { href: "https://www.thomasnet.com" + path, hostname: "www.thomasnet.com", pathname: path },
    writable: true,
  });
  document.head.innerHTML = head;
  document.body.innerHTML = body;
}

describe("ThomasNet page copy", () => {
  it("keeps the data script but no other script", () => {
    const doc = new DOMParser().parseFromString(
      `<html><body><h1>Applied Rapid Technologies</h1><script>track()</script>${DATA}</body></html>`,
      "text/html",
    );
    const html = scrapers.thomasnetTrimmed(doc);
    expect(html).toContain('id="__NEXT_DATA__"');
    expect(html).toContain("Applied Rapid Technologies");
    expect(html).not.toContain("track()");
  });
});

describe("ThomasNet page from the browser", () => {
  it("loads the page again", async () => {
    onPage(PATH, "", "<h1>old</h1>");
    const fetchMock = vi.fn().mockResolvedValue(new Response(`<html><body><h1>New</h1>${DATA}</body></html>`, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const html = await scrapers.thomasnetPageHtml();
    expect(html).toContain("New");
    expect(html).toContain("__NEXT_DATA__");
    expect(fetchMock).toHaveBeenCalledWith("https://www.thomasnet.com" + PATH, expect.anything());
    vi.unstubAllGlobals();
  });

  it("copies the tab when loading fails, if the tab is the same company", async () => {
    onPage(PATH, `<link rel="canonical" href="https://www.thomasnet.com${PATH}">`, `<h1>Tab copy</h1>${DATA}`);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(await scrapers.thomasnetPageHtml()).toContain("Tab copy");
    vi.unstubAllGlobals();
  });

  it("sends nothing when the tab still shows another company's data", async () => {
    onPage(
      "/company/other-company-123/profile",
      `<link rel="canonical" href="https://www.thomasnet.com${PATH}">`,
      `<h1>Tab copy</h1>${DATA}`,
    );
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(await scrapers.thomasnetPageHtml()).toBe(null);
    vi.unstubAllGlobals();
  });
});
