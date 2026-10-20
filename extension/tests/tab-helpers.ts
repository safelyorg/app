import { vi } from "vitest";
import { fakeChrome } from "./setup-chrome";

// The extension's scripts are plain browser scripts that share a few
// globals from core/state.ts (escapeHtml and the ✓ / ✕ icons). In the
// browser every script sees them; in a test each file is its own
// module, so they are put on the window here, exactly as state.ts
// defines them.
export const CHECK_ICON =
  '<svg class="safely-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>';
export const CROSS_ICON =
  '<svg class="safely-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17"/></svg>';

export function installSharedGlobals(): void {
  (globalThis as any).SAFELY_CHECK_ICON = CHECK_ICON;
  (globalThis as any).SAFELY_CROSS_ICON = CROSS_ICON;
  (globalThis as any).escapeHtml = (str: string): string => {
    const div = document.createElement("div");
    div.textContent = str;
    return div.innerHTML;
  };
  // No WASM in tests: the tabs use their plain-JS score bands.
  (fakeChrome.runtime as any).getURL = vi.fn((path: string) => {
    if (path.indexOf("pkg/") === 0) throw new Error("no WASM in tests");
    return "chrome-extension://test/" + path;
  });
  // Storage with the callback form the scripts use.
  (fakeChrome.storage.local as any).get = vi.fn((_key: unknown, cb?: (r: any) => void) => {
    if (cb) cb({});
    return Promise.resolve({});
  });
  (fakeChrome.storage as any).onChanged = { addListener: vi.fn() };
}

// A stand-in for panel.ts: the panel and one element per tab, and an
// __safelyAddTab that puts the tab's HTML straight into its element.
export function mountFakePanel(tabIds: string[]): void {
  document.body.innerHTML =
    '<div id="safely-root"><div id="safely-panel">' +
    tabIds.map((id) => '<div id="safely-tab-' + id + '"></div>').join("") +
    "</div></div>";
  (window as any).__safelyAddTab = (
    id: string,
    _title: string,
    html: string,
    _icon: string,
    init: ((root: HTMLElement) => void) | null,
  ) => {
    const el = document.getElementById("safely-tab-" + id) as HTMLElement;
    el.innerHTML = html;
    if (init) init(document.body);
  };
}

// Runs one of the extension's real scripts ("../ts/tabs/risk"). They
// are plain browser scripts, not modules, so the path is passed as a
// plain string - TypeScript can't type an import of a non-module.
export function runScript(path: string): Promise<unknown> {
  return import(/* @vite-ignore */ path);
}

// A file's text. The path is from the extension folder, where the
// tests run. Node's file tools are loaded by name, so the editor needs
// no Node types.
const NODE_FS = "node:fs";
const NODE_PATH = "node:path";

export async function readText(file: string): Promise<string> {
  const fs: any = await import(/* @vite-ignore */ NODE_FS);
  const path: any = await import(/* @vite-ignore */ NODE_PATH);
  const root: string = (globalThis as any).process.cwd();
  return fs.readFileSync(path.resolve(root, file), "utf8");
}

export function tab(id: string): HTMLElement {
  return document.getElementById("safely-tab-" + id) as HTMLElement;
}

// A supplier scan as api.ts stores it in __safelyData.
export function supplierData(overrides: Record<string, any> = {}): any {
  return {
    analysisId: "analysis-1",
    riskScore: 20,
    fraudReportCount: 0,
    reportedByYou: false,
    signals: [],
    riskFactors: [],
    socialCandidates: [],
    ...overrides,
    seller: {
      name: "Palmer Holland",
      platform: "ThomasNet",
      platformId: "palmer-holland",
      accountAge: "About 11 years",
      verification: "unknown",
      phone: "Not found",
      handle: "",
      location: "Ohio, USA",
      lastActive: "Unknown",
      monthlyActivity: [],
      ...(overrides.seller || {}),
    },
  };
}

// Shows a new result, the way api.ts does after a scan.
export function showResult(data: any): void {
  (window as any).__safelyData = data;
  window.dispatchEvent(new CustomEvent("safely-data-ready"));
}

export function signal(label: string, value: string, type: string, sub = ""): any {
  return { label, value, type, sub };
}
