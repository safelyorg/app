import { describe, it, expect, beforeAll, beforeEach, vi } from "vitest";
import { loadScript, global, readText } from "./load-script";

// Light is the default; Settings > Appearance switches to Dark. These
// tests load the real ts/settings.ts and the small script at the top
// of index.html, and check the logo rules in css/dashboard.css.

let page = "";
let css = "";

// The script in index.html that sets the theme before the page draws.
function runHeadThemeScript(): void {
  const scripts = [...page.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
  const themeScript = scripts.find((s) => s.indexOf("safely_theme") !== -1);
  if (!themeScript) throw new Error("index.html has no theme script");
  (0, eval)(themeScript);
}

beforeAll(async () => {
  page = await readText("index.html");
  css = (await readText("css/dashboard.css")).replace(/\s+/g, " ");
  await loadScript("ts/shared.ts");
  await loadScript("ts/settings.ts");
});

beforeEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  document.body.innerHTML =
    '<button class="theme-btn" data-theme-choice="light">Light</button>' +
    '<button class="theme-btn" data-theme-choice="dark">Dark</button>';
});

const button = (choice: string) =>
  document.querySelector('[data-theme-choice="' + choice + '"]') as HTMLButtonElement;

describe("theme - before the page draws", () => {
  it("starts in Light when nothing was chosen", () => {
    runHeadThemeScript();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("starts in Dark when Dark was chosen before", () => {
    localStorage.setItem("safely_theme", "dark");
    runHeadThemeScript();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("falls back to Light for anything else", () => {
    localStorage.setItem("safely_theme", "system");
    runHeadThemeScript();
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});

describe("theme - Settings > Appearance", () => {
  it("marks Light as chosen by default", () => {
    global("initThemeSwitch")();
    expect(button("light").classList.contains("is-active")).toBe(true);
    expect(button("light").getAttribute("aria-pressed")).toBe("true");
    expect(button("dark").getAttribute("aria-pressed")).toBe("false");
  });

  it("switches to Dark and remembers it", () => {
    global("initThemeSwitch")();
    button("dark").click();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("safely_theme")).toBe("dark");
    expect(button("dark").classList.contains("is-active")).toBe(true);
    expect(button("light").classList.contains("is-active")).toBe(false);
  });

  it("switches back to Light", () => {
    global("initThemeSwitch")();
    button("dark").click();
    button("light").click();
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("safely_theme")).toBe("light");
  });

  it("still switches when the browser can't save (private window)", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    global("initThemeSwitch")();
    button("dark").click();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});

describe("theme - logo", () => {
  it("shows the dark logo in Light and the light logo in Dark", () => {
    expect(css).toContain(".logo-for-dark { display: none; }");
    expect(css).toContain(':root[data-theme="dark"] .logo-for-light { display: none; }');
    expect(css).toContain(':root[data-theme="dark"] .logo-for-dark { display: block; }');
  });

  it("uses both logo pictures in the page", () => {
    expect(page).toMatch(/src="images\/logo-dark\.png"[^>]*class="logo-for-light/);
    expect(page).toMatch(/src="images\/logo-light\.png"[^>]*class="logo-for-dark/);
  });
});
