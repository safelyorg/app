import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeChrome } from "./setup-chrome";
import { installSharedGlobals, runScript } from "./tab-helpers";

// These tests load the real core/panel.ts: the status dot on the
// floating button, the waiting screen, the packed fonts, and the rows
// that open / "Show more" / ticks shared by both tabs.

let signedIn = true;

// panel.ts watches the page for URL changes. Each test loads a fresh
// panel, so the old watchers are stopped after every test - otherwise
// they keep firing after the test page is gone ("window is not defined").
const watchers: MutationObserver[] = [];
const RealMutationObserver = globalThis.MutationObserver;
class TrackedMutationObserver extends RealMutationObserver {
  constructor(callback: MutationCallback) {
    super(callback);
    watchers.push(this);
  }
}
(globalThis as any).MutationObserver = TrackedMutationObserver;

async function loadPanel(): Promise<void> {
  document.body.innerHTML = "";
  vi.resetModules();
  installSharedGlobals();
  (fakeChrome.storage.local as any).get = vi.fn((_key: unknown, cb?: (r: any) => void) => {
    const result = signedIn ? { safely_session_token: "token" } : {};
    if (cb) cb(result);
    return Promise.resolve(result);
  });
  (window as any).__safelyAPI = {
    SITE_BASE: "https://safely.sh",
    fetchAnalysis: vi.fn(),
    getScanUsage: vi.fn().mockResolvedValue(null),
  };
  (window as any).__safelyScrapers = { isListingPage: vi.fn().mockReturnValue(true) };
  (window as any).__safelyResetState = vi.fn();
  (globalThis as any).__safelyFormatUsageLine = () => "";
  (globalThis as any).fetch = vi.fn().mockResolvedValue({
    arrayBuffer: async () => new ArrayBuffer(0),
  });
  await runScript("../ts/core/panel");
  // Register a Risk tab, as tabs/risk.ts does.
  (window as any).__safelyAddTab("risk", "Risk", "<p>risk tab</p>", "<svg></svg>", null);
}

const dot = () => document.getElementById("safely-status-dot") as HTMLElement;
const toolbar = () => document.getElementById("safely-toolbar") as HTMLElement;
const finish = (detail: any = {}) =>
  window.dispatchEvent(new CustomEvent("safely-analysis-finished", { detail }));

beforeEach(() => {
  signedIn = true;
  vi.useFakeTimers();
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  watchers.splice(0).forEach((w) => w.disconnect());
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("panel - status dot", () => {
  it("pulses while a check runs", async () => {
    await loadPanel();
    expect(dot().classList.contains("safely-dot-run")).toBe(true);
    expect(toolbar().title).toBe("Checking this supplier");
  });

  it("turns 'done' when the result arrives and the panel is closed", async () => {
    await loadPanel();
    finish();
    expect(dot().classList.contains("safely-dot-done")).toBe(true);
    expect(dot().classList.contains("safely-dot-run")).toBe(false);
    expect(toolbar().title).toBe("Result ready");
  });

  it("clears once the panel is opened", async () => {
    await loadPanel();
    finish();
    (document.querySelector('[data-open="risk"]') as HTMLElement).click();
    expect(dot().className).toBe("safely-status-dot");
    expect(toolbar().title).toBe("");
  });

  it("stays clear when the result arrives while the panel is open", async () => {
    await loadPanel();
    (document.querySelector('[data-open="risk"]') as HTMLElement).click();
    finish();
    expect(dot().className).toBe("safely-status-dot");
  });

  it("shows nothing on a page that isn't a listing", async () => {
    (window as any).__safelyScrapers = { isListingPage: () => false };
    document.body.innerHTML = "";
    vi.resetModules();
    installSharedGlobals();
    (window as any).__safelyAPI = { SITE_BASE: "", fetchAnalysis: vi.fn(), getScanUsage: vi.fn() };
    (globalThis as any).fetch = vi.fn().mockResolvedValue({ arrayBuffer: async () => new ArrayBuffer(0) });
    await runScript("../ts/core/panel");
    expect(dot().className).toBe("safely-status-dot");
  });

  it("shows nothing when you are signed out", async () => {
    signedIn = false;
    await loadPanel();
    expect(dot().className).toBe("safely-status-dot");
  });
});

describe("panel - waiting screen", () => {
  it("lists the steps and moves to the next one over time", async () => {
    await loadPanel();
    const steps = () => document.querySelectorAll("#safely-checking .safely-step");
    expect(steps()).toHaveLength(7);
    expect(steps()[0].classList.contains("safely-now")).toBe(true);
    expect(document.getElementById("safely-wait-now")?.textContent).toBe("Reading the listing");

    vi.advanceTimersByTime(1400);
    expect(steps()[0].classList.contains("safely-done")).toBe(true);
    expect(steps()[0].querySelector("svg")).not.toBeNull();
    expect(steps()[1].classList.contains("safely-now")).toBe(true);
  });

  it("never runs past the last step while waiting for the answer", async () => {
    await loadPanel();
    vi.advanceTimersByTime(60_000);
    expect(document.getElementById("safely-wait-now")?.textContent).toBe("Working out the score");
  });

  it("goes away when the result arrives", async () => {
    await loadPanel();
    finish();
    const checking = document.getElementById("safely-checking") as HTMLElement;
    expect(checking.classList.contains("safely-visible")).toBe(false);
    expect(checking.innerHTML).toBe("");
  });
});

describe("panel - fonts", () => {
  it("loads the six packed fonts from the extension", async () => {
    await loadPanel();
    const files = ((fakeChrome.runtime as any).getURL as any).mock.calls.map((c: any[]) => c[0]);
    expect(files).toEqual(
      expect.arrayContaining([
        "fonts/inter-400.woff2",
        "fonts/inter-500.woff2",
        "fonts/inter-600.woff2",
        "fonts/inter-700.woff2",
        "fonts/inter-800.woff2",
        "fonts/gasoek-one.woff2",
      ]),
    );
    expect((globalThis as any).fetch).toHaveBeenCalledTimes(6);
  });
});

describe("panel - rows that open, Show more, ticks", () => {
  function addList(): HTMLElement {
    const area = document.getElementById("safely-tabs-area") as HTMLElement;
    const list = document.createElement("div");
    list.className = "safely-rows";
    list.innerHTML =
      '<div class="safely-row"><button data-safely-open aria-expanded="false"><span>Row</span></button></div>' +
      '<div class="safely-row"><span data-safely-tick="Order a sample first" aria-checked="false"></span></div>' +
      '<button data-safely-more="Show 2 more"><span>Show 2 more</span></button>';
    area.appendChild(list);
    return list;
  }

  it("opens and closes a row", async () => {
    await loadPanel();
    const list = addList();
    const opener = list.querySelector("[data-safely-open] span") as HTMLElement;
    opener.click();
    expect(list.children[0].classList.contains("safely-open")).toBe(true);
    expect(list.querySelector("[data-safely-open]")?.getAttribute("aria-expanded")).toBe("true");
    opener.click();
    expect(list.children[0].classList.contains("safely-open")).toBe(false);
  });

  it("shows all rows and switches to 'Show less', then back", async () => {
    await loadPanel();
    const list = addList();
    const more = list.querySelector("[data-safely-more]") as HTMLElement;
    more.click();
    expect(list.classList.contains("safely-show-all")).toBe(true);
    expect(more.textContent).toBe("Show less");
    more.click();
    expect(list.classList.contains("safely-show-all")).toBe(false);
    expect(more.textContent).toBe("Show 2 more");
  });

  it("ticks and unticks a step", async () => {
    await loadPanel();
    const list = addList();
    const tick = list.querySelector("[data-safely-tick]") as HTMLElement;
    tick.click();
    expect(list.children[1].classList.contains("safely-ticked")).toBe(true);
    expect(tick.getAttribute("aria-checked")).toBe("true");
    tick.click();
    expect(tick.getAttribute("aria-checked")).toBe("false");
  });
});
