// Ad tracking for the website (Meta Pixel, loaded in each page's <head>):
// 1. Remembers where a visitor came from (utm_source, utm_medium,
//    utm_campaign, utm_content in the link of a Meta ad), so the
//    dashboard can save it with the account at sign-up.
// 2. Sends a "Lead" event when someone clicks an "Add to Chrome" /
//    "Adicionar ao Chrome" button or any link to the Chrome Web Store.
// Does nothing when the Pixel is not loaded (blocked by the browser).
(function () {
  "use strict";

  // Same key the dashboard reads (same website, so same storage).
  const UTM_STORAGE_KEY = "safely_utm";
  const UTM_KEYS = ["utm_source", "utm_medium", "utm_campaign", "utm_content"];

  // The FIRST ad a visitor came from is kept: a later visit without
  // UTMs (or from another ad) does not overwrite it.
  function rememberAdSource(): void {
    try {
      if (localStorage.getItem(UTM_STORAGE_KEY)) return;
      const params = new URLSearchParams(window.location.search);
      const found: Record<string, string> = {};
      UTM_KEYS.forEach((key) => {
        const value = params.get(key);
        if (value) found[key] = value.slice(0, 200);
      });
      if (Object.keys(found).length === 0) return;
      found.landed_at = new Date().toISOString();
      localStorage.setItem(UTM_STORAGE_KEY, JSON.stringify(found));
    } catch (e) {
      // Storage blocked (private mode): nothing is remembered.
    }
  }

  function track(event: string): void {
    const fbq = (window as any).fbq;
    if (typeof fbq === "function") fbq("track", event);
  }

  function isChromeStoreLink(link: HTMLAnchorElement): boolean {
    const href = link.getAttribute("href") || "";
    return href.indexOf("chromewebstore.google.com") !== -1 || href.indexOf("chrome.google.com/webstore") !== -1;
  }

  // One listener for the whole page, so every button counts - also ones
  // added later or translated.
  document.addEventListener("click", (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;
    const link = target ? (target.closest("a") as HTMLAnchorElement | null) : null;
    if (!link || !isChromeStoreLink(link)) return;

    track("Lead");

    // A link that opens in the same tab leaves the page at once; a short
    // wait lets the event reach Meta first. New-tab links need no wait.
    const opensNewTab = link.target === "_blank" || event.ctrlKey || event.metaKey || event.shiftKey;
    if (!opensNewTab && typeof (window as any).fbq === "function") {
      event.preventDefault();
      const href = link.href;
      setTimeout(() => {
        window.location.href = href;
      }, 300);
    }
  });

  rememberAdSource();
})();
