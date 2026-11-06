import { describe, it, expect, beforeEach } from "vitest";
import { loadScripts } from "./load-script";

// The "Add to Chrome" box and the popup after installing
// (ts/install_welcome.ts).

async function start(address: string): Promise<void> {
  window.history.replaceState(null, "", address);
  document.body.innerHTML =
    '<a id="get-extension-btn" href="https://chromewebstore.google.com/detail/safely/x">Add to Chrome</a>';
  await loadScripts(["ts/shared.ts", "ts/install_welcome.ts"]);
}

const popup = () => document.getElementById("safely-installed-popup");

describe("install welcome", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.innerHTML = "";
  });

  it("remembers the dashboard page when the button is clicked", async () => {
    await start("/reports");
    (document.getElementById("get-extension-btn") as HTMLAnchorElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
    const saved = JSON.parse(localStorage.getItem("safely_install_from") || "{}");
    expect(new URL(saved.url).pathname).toBe("/reports");
    expect(popup()).toBeNull();
  });

  it("shows the popup after installing and cleans the address", async () => {
    await start("/dashboard?installed=1");
    expect(popup()?.textContent).toContain("Thanks for installing Safely!");
    expect(window.location.search).toBe("");
    (popup()!.querySelector("button") as HTMLButtonElement).click();
    expect(popup()).toBeNull();
  });

  it("shows the popup on the page the click came from", async () => {
    localStorage.setItem(
      "safely_install_from",
      JSON.stringify({ url: window.location.origin + "/dashboard", at: Date.now() }),
    );
    await start("/dashboard?installed=1");
    expect(popup()).not.toBeNull();
    expect(localStorage.getItem("safely_install_from")).toBeNull();
  });

  it("tells the Meta Pixel only on safely.sh, never on a local test", async () => {
    const calls: unknown[][] = [];
    (window as any).fbq = (...args: unknown[]) => calls.push(args);
    await start("/dashboard?installed=1");
    // The tests run on localhost: nothing is sent.
    expect(calls).toEqual([]);
    delete (window as any).fbq;
  });

  it("does nothing on a normal visit", async () => {
    await start("/dashboard");
    expect(popup()).toBeNull();
  });
});
