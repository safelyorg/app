// ── Back to the page after installing the extension ──
// The "Add to Chrome" box in the sidebar (index.html, #get-extension-btn)
// remembers the page it was clicked on. Chrome does not send anyone back
// after installing, so the extension opens safely.sh with "?installed=1"
// (extension background.ts). The website's tracking script sends the
// visitor back here when the click was on the dashboard; this file then
// shows a "thank you for installing" popup. The website uses the same
// "safely_install_from" key (site ts/tracking.ts), so a click on either
// side leads back to the right page.
(function () {
  const INSTALL_FROM_KEY = "safely_install_from";
  // A click older than this is not the one that led to the install.
  const INSTALL_FROM_MAX_AGE_MS = 6 * 60 * 60 * 1000;

  // This page's address without "installed=1".
  function pageWithoutInstalled(): string {
    const url = new URL(window.location.href);
    url.searchParams.delete("installed");
    return url.toString();
  }

  function rememberInstallPage(): void {
    try {
      localStorage.setItem(
        INSTALL_FROM_KEY,
        JSON.stringify({ url: pageWithoutInstalled(), at: Date.now() }),
      );
    } catch (e) {
      // Storage blocked: after installing, the home page shows the popup.
    }
  }

  // The page the button was clicked on, if recent (and on this site).
  function takeInstallPage(): string | null {
    try {
      const saved = JSON.parse(localStorage.getItem(INSTALL_FROM_KEY) || "null");
      localStorage.removeItem(INSTALL_FROM_KEY);
      if (!saved || typeof saved.url !== "string" || Date.now() - saved.at > INSTALL_FROM_MAX_AGE_MS) {
        return null;
      }
      const url = new URL(saved.url);
      return url.origin === window.location.origin ? url.toString() : null;
    } catch (e) {
      return null;
    }
  }

  const TEXTS: [string, string, string][] = [
    ["title", "dash.installed.title", "Thanks for installing Safely!"],
    [
      "body",
      "dash.installed.body",
      "You're all set. Open any supplier page on Alibaba, Made-in-China, Global Sources or another supported site, and the check starts by itself.",
    ],
    [
      "tip",
      "dash.installed.tip",
      "Tip: click the puzzle icon in Chrome's toolbar and pin Safely, so it is always in sight.",
    ],
    ["button", "dash.installed.button", "Got it"],
  ];

  // Fills the popup's texts in the dashboard's language (again once the
  // translations have loaded).
  function fillTexts(popup: HTMLElement): void {
    TEXTS.forEach(([part, key, fallback]) => {
      const el = popup.querySelector('[data-part="' + part + '"]');
      if (el) el.textContent = t(key, fallback);
    });
    popup.setAttribute("aria-label", t("dash.installed.title", "Thanks for installing Safely!"));
  }

  function showInstalledPopup(): void {
    const popup = document.createElement("div");
    popup.id = "safely-installed-popup";
    popup.setAttribute("role", "dialog");
    popup.setAttribute("aria-modal", "true");
    popup.className = "fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/60";
    popup.innerHTML =
      '<div class="w-full max-w-sm bg-surface border border-line rounded-2xl shadow-2xl p-6 text-center">' +
      '<div class="w-12 h-12 mx-auto mb-4 rounded-xl bg-mint/15 text-mint flex items-center justify-center">' +
      '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>' +
      "</div>" +
      '<h2 data-part="title" class="text-lg font-extrabold text-ink tracking-tight mb-2"></h2>' +
      '<p data-part="body" class="text-[13px] leading-relaxed text-muted mb-3"></p>' +
      '<p data-part="tip" class="text-[12px] leading-snug text-ink bg-surface2 border border-line rounded-lg px-3 py-2.5 mb-5"></p>' +
      '<button data-part="button" type="button" class="w-full py-2.5 rounded-lg bg-brand text-surface text-[13px] font-bold hover:opacity-90 transition"></button>' +
      "</div>";
    fillTexts(popup);
    const close = (): void => {
      popup.remove();
      document.removeEventListener("keydown", onKey);
    };
    function onKey(e: KeyboardEvent): void {
      if (e.key === "Escape") close();
    }
    (popup.querySelector("button") as HTMLButtonElement).addEventListener("click", close);
    popup.addEventListener("click", (e) => {
      if (e.target === popup) close();
    });
    document.addEventListener("keydown", onKey);
    window.addEventListener("safely-i18n-ready", () => fillTexts(popup));
    document.body.appendChild(popup);
    (popup.querySelector("button") as HTMLButtonElement).focus();
  }

  // Tells the Meta Pixel the extension was installed: a custom event,
  // "ExtensionInstalled" (same as the website). Sent once, with the
  // popup, and only from safely.sh, so local tests never count.
  function trackInstalled(): void {
    if (window.location.hostname !== "safely.sh") return;
    const fbq = (window as any).fbq;
    if (typeof fbq === "function") fbq("trackCustom", "ExtensionInstalled");
  }

  // "?installed=1": go back to the page the button was clicked on, or -
  // already there, or no click remembered - show the popup here.
  function handleInstalled(): void {
    if (new URLSearchParams(window.location.search).get("installed") !== "1") return;
    const here = pageWithoutInstalled();
    const from = takeInstallPage();
    if (from && from.split("#")[0] !== here.split("#")[0]) {
      const back = new URL(from);
      back.searchParams.set("installed", "1");
      window.location.replace(back.toString());
      return;
    }
    history.replaceState(null, "", here);
    trackInstalled();
    showInstalledPopup();
  }

  function start(): void {
    const button = document.getElementById("get-extension-btn");
    if (button) {
      button.addEventListener("click", () => {
        rememberInstallPage();
        // Counts as a Lead for the Meta Pixel, like the website's buttons.
        const fbq = (window as any).fbq;
        if (typeof fbq === "function") fbq("track", "Lead");
      });
    }
    handleInstalled();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
