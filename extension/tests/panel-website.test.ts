import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeChrome } from "./setup-chrome";
import { installSharedGlobals, runScript } from "./tab-helpers";

// The panel on an ordinary website: its own icon (a magnifying glass,
// not the marketplace "not a listing" note), and a check that keeps
// going while the shop changes its own address ("?variant=2").

// ---- The button in the panel --------------------------------------

const watchers: MutationObserver[] = [];
const RealMutationObserver = globalThis.MutationObserver;
class TrackedMutationObserver extends RealMutationObserver {
  constructor(callback: MutationCallback) {
    super(callback);
    watchers.push(this);
  }
}
(globalThis as any).MutationObserver = TrackedMutationObserver;

async function loadPanel(websiteCheckable: boolean, marketplaceMessage: string | null) {
  document.body.innerHTML = "";
  vi.resetModules();
  installSharedGlobals();
  (fakeChrome.storage.local as any).get = vi.fn((_key: unknown, cb?: (r: any) => void) => {
    const result = { safely_session_token: "token" };
    if (cb) cb(result);
    return Promise.resolve(result);
  });
  const api = {
    SITE_BASE: "https://safely.sh",
    fetchAnalysis: vi.fn(),
    checkWebsite: vi.fn(),
    getScanUsage: vi.fn().mockResolvedValue(null),
  };
  (window as any).__safelyAPI = api;
  (window as any).__safelyScrapers = {
    isListingPage: vi.fn().mockReturnValue(false),
    isWebsiteCheckable: vi.fn().mockReturnValue(websiteCheckable),
    getUnsupportedMessage: vi.fn().mockReturnValue(marketplaceMessage),
  };
  (window as any).__safelyResetState = vi.fn();
  (globalThis as any).__safelyFormatUsageLine = () => "";
  (globalThis as any).fetch = vi.fn().mockResolvedValue({ arrayBuffer: async () => new ArrayBuffer(0) });
  await runScript("../ts/core/panel");
  (window as any).__safelyAddTab("risk", "Risk", "<p>risk tab</p>", "<svg></svg>", null);
  return api;
}

const MAGNIFIER = '<circle cx="11" cy="11" r="8"></circle>';

describe("panel on an ordinary website", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, "warn").mockImplementation(() => {});
    history.replaceState(null, "", "/products/phone");
  });
  afterEach(() => {
    watchers.splice(0).forEach((w) => w.disconnect());
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("shows a magnifying glass instead of the 'not a listing' icon", async () => {
    await loadPanel(true, null);
    expect(document.getElementById("safely-unsupported-state-icon")!.innerHTML).toContain(MAGNIFIER);
    const toolbarIcon = document.querySelector('[data-open="unsupported"]') as HTMLElement;
    expect(toolbarIcon.innerHTML).toContain(MAGNIFIER);
    expect(toolbarIcon.title).toBe("Check this website");
  });

  it("keeps the 'not a listing' icon on a marketplace page", async () => {
    await loadPanel(false, "Open a product page on this marketplace.");
    expect(document.getElementById("safely-unsupported-state-icon")!.innerHTML).not.toContain(MAGNIFIER);
    expect((document.querySelector('[data-open="unsupported"]') as HTMLElement).title).toBe("Not a listing page");
  });

  it("keeps waiting for the result when the shop changes its own address", async () => {
    const api = await loadPanel(true, null);
    (document.getElementById("safely-check-website-btn") as HTMLElement).click();
    const checking = document.getElementById("safely-checking")!;
    expect(checking.classList.contains("safely-visible")).toBe(true);

    history.pushState(null, "", "/products/phone?variant=2");
    document.body.appendChild(document.createElement("span"));
    await vi.advanceTimersByTimeAsync(10);

    expect(checking.classList.contains("safely-visible")).toBe(true);
    expect(document.getElementById("safely-tab-unsupported")!.style.display).toBe("none");
    expect(api.checkWebsite).toHaveBeenCalledTimes(1);
  });
});
