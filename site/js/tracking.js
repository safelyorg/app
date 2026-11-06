"use strict";
// Ad tracking for the website (Meta Pixel, loaded in each page's <head>):
// 1. Remembers where a visitor came from (utm_source, utm_medium,
//    utm_campaign, utm_content in the link of a Meta ad), so the
//    dashboard can save it with the account at sign-up.
// 2. Sends a "Lead" event when someone clicks an "Add to Chrome" /
//    "Adicionar ao Chrome" button or any link to the Chrome Web Store.
// 3. Adds where the visitor came from to that Chrome Web Store link, so
//    the store's own Google Analytics shows it: the Meta ad they came
//    from, or else "website / button / <page>" (e.g. "homepage", "try").
// 4. Remembers the page the button was clicked on. Chrome does not send
//    anyone back after installing, so the extension opens safely.sh with
//    "?installed=1" (background.ts); this script then goes back to that
//    page and shows a "thank you for installing" popup there. The
//    dashboard does the same (dashboard/ts/install_welcome.ts) - both
//    use the same "safely_install_from" key, so a click on either side
//    leads back to the right page.
// Does nothing when the Pixel is not loaded (blocked by the browser),
// except steps 3 and 4, which never need the Pixel.
(function () {
    "use strict";
    // Same key the dashboard reads (same website, so same storage).
    const UTM_STORAGE_KEY = "safely_utm";
    const UTM_KEYS = ["utm_source", "utm_medium", "utm_campaign", "utm_content"];
    // The FIRST ad a visitor came from is kept: a later visit without
    // UTMs (or from another ad) does not overwrite it.
    function rememberAdSource() {
        try {
            if (localStorage.getItem(UTM_STORAGE_KEY))
                return;
            const params = new URLSearchParams(window.location.search);
            const found = {};
            UTM_KEYS.forEach((key) => {
                const value = params.get(key);
                if (value)
                    found[key] = value.slice(0, 200);
            });
            if (Object.keys(found).length === 0)
                return;
            found.landed_at = new Date().toISOString();
            localStorage.setItem(UTM_STORAGE_KEY, JSON.stringify(found));
        }
        catch (e) {
            // Storage blocked (private mode): nothing is remembered.
        }
    }
    // The ad this visitor first came from (see rememberAdSource), or null.
    function rememberedAdSource() {
        try {
            const saved = JSON.parse(localStorage.getItem(UTM_STORAGE_KEY) || "null");
            return saved && saved.utm_source ? saved : null;
        }
        catch (e) {
            return null;
        }
    }
    // The page the button is on, as a short name: "/" -> "homepage",
    // "/try" -> "try", "/tools/domain-age-checker" -> "tools_domain-age-checker".
    function pageName() {
        const path = window.location.pathname.replace(/^\/+|\/+$/g, "").replace(/\.html$/, "");
        return path ? path.replace(/\//g, "_") : "homepage";
    }
    // The Chrome Web Store link with where the visitor came from added.
    // A link that already says where it came from is left as it is.
    function withSource(href) {
        try {
            const url = new URL(href, window.location.href);
            if (url.searchParams.has("utm_source"))
                return href;
            const ad = rememberedAdSource();
            if (ad) {
                UTM_KEYS.forEach((key) => {
                    if (ad[key])
                        url.searchParams.set(key, ad[key]);
                });
            }
            else {
                url.searchParams.set("utm_source", "website");
                url.searchParams.set("utm_medium", "button");
                url.searchParams.set("utm_campaign", pageName());
            }
            return url.toString();
        }
        catch (e) {
            return href;
        }
    }
    // ---- Back to the page after installing ----------------------------
    const INSTALL_FROM_KEY = "safely_install_from";
    // A click older than this is not the one that led to the install.
    const INSTALL_FROM_MAX_AGE_MS = 6 * 60 * 60 * 1000;
    // This page's address without "installed=1".
    function pageWithoutInstalled() {
        const url = new URL(window.location.href);
        url.searchParams.delete("installed");
        return url.toString();
    }
    function rememberInstallPage() {
        try {
            localStorage.setItem(INSTALL_FROM_KEY, JSON.stringify({ url: pageWithoutInstalled(), at: Date.now() }));
        }
        catch (e) {
            // Storage blocked: after installing, the home page shows the popup.
        }
    }
    // The page the "Add to Chrome" button was clicked on, if recent.
    function takeInstallPage() {
        try {
            const saved = JSON.parse(localStorage.getItem(INSTALL_FROM_KEY) || "null");
            localStorage.removeItem(INSTALL_FROM_KEY);
            if (!saved || typeof saved.url !== "string" || Date.now() - saved.at > INSTALL_FROM_MAX_AGE_MS) {
                return null;
            }
            const url = new URL(saved.url);
            return url.origin === window.location.origin ? url.toString() : null;
        }
        catch (e) {
            return null;
        }
    }
    function isPortuguese() {
        try {
            const saved = localStorage.getItem("safely_lang");
            if (saved)
                return saved === "pt-br";
        }
        catch (e) {
            // Storage blocked: the browser's language decides.
        }
        return (navigator.language || "").toLowerCase().startsWith("pt");
    }
    function showInstalledPopup() {
        const pt = isPortuguese();
        const text = pt
            ? {
                title: "Obrigado por instalar o Safely!",
                body: "Está tudo pronto. Abra a página de um fornecedor no Alibaba, Made-in-China, Global Sources ou outro site compatível, e a verificação começa sozinha.",
                tip: "Dica: clique no ícone de quebra-cabeça do Chrome e fixe o Safely para ele ficar sempre à vista.",
                button: "Entendi",
            }
            : {
                title: "Thanks for installing Safely!",
                body: "You're all set. Open any supplier page on Alibaba, Made-in-China, Global Sources or another supported site, and the check starts by itself.",
                tip: "Tip: click the puzzle icon in Chrome's toolbar and pin Safely, so it is always in sight.",
                button: "Got it",
            };
        const overlay = document.createElement("div");
        overlay.id = "safely-installed-popup";
        overlay.setAttribute("role", "dialog");
        overlay.setAttribute("aria-modal", "true");
        overlay.setAttribute("aria-label", text.title);
        overlay.style.cssText =
            "position:fixed;inset:0;z-index:2147483000;display:flex;align-items:center;justify-content:center;padding:16px;background:rgba(22,22,22,.55);";
        overlay.innerHTML =
            '<div style="max-width:420px;width:100%;background:var(--paper,#fff);color:var(--ink,#161616);border:var(--bd,3px solid #161616);border-radius:var(--r-lg,24px);box-shadow:var(--sh-lg,8px 8px 0 #161616);padding:28px 26px 24px;text-align:center;font-family:inherit;">' +
                '<div style="width:56px;height:56px;margin:0 auto 14px;border-radius:16px;border:var(--bd-thin,2px solid #161616);background:var(--mint,#7ee0b5);display:flex;align-items:center;justify-content:center;">' +
                '<svg width="28" height="28" viewBox="0 0 24 24" fill="none"><path d="M6 12.5l4 4 8-9" stroke="#161616" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg></div>' +
                '<h2 style="font-family:var(--disp,inherit);font-weight:800;font-size:22px;line-height:1.2;margin:0 0 10px;"></h2>' +
                '<p data-part="body" style="font-size:15px;line-height:1.55;margin:0 0 12px;color:var(--muted,#555);font-weight:500;"></p>' +
                '<p data-part="tip" style="font-size:13px;line-height:1.5;margin:0 0 20px;padding:10px 12px;border-radius:12px;background:var(--cream,#fbf7ed);color:var(--ink,#161616);font-weight:600;"></p>' +
                '<button type="button" style="cursor:pointer;width:100%;padding:12px 18px;border-radius:14px;border:var(--bd,3px solid #161616);background:var(--sun,#ffd84d);color:#161616;font-weight:800;font-size:15px;box-shadow:var(--sh,4px 4px 0 #161616);font-family:inherit;"></button>' +
                "</div>";
        overlay.querySelector("h2").textContent = text.title;
        overlay.querySelector('[data-part="body"]').textContent = text.body;
        overlay.querySelector('[data-part="tip"]').textContent = text.tip;
        const button = overlay.querySelector("button");
        button.textContent = text.button;
        const close = () => overlay.remove();
        button.addEventListener("click", close);
        overlay.addEventListener("click", (e) => {
            if (e.target === overlay)
                close();
        });
        document.addEventListener("keydown", function onKey(e) {
            if (e.key === "Escape") {
                close();
                document.removeEventListener("keydown", onKey);
            }
        });
        document.body.appendChild(overlay);
        button.focus();
    }
    // Tells the Meta Pixel the extension was installed: a custom event,
    // "ExtensionInstalled" (Meta has no standard one for this). Sent once,
    // on the page that shows the thank-you popup, and only from safely.sh,
    // so local tests (localhost) never count as installs.
    function trackInstalled() {
        if (window.location.hostname !== "safely.sh")
            return;
        const fbq = window.fbq;
        if (typeof fbq === "function")
            fbq("trackCustom", "ExtensionInstalled");
    }
    // "?installed=1" (opened by the extension right after installing): go
    // back to the page the button was clicked on, or - already there, or
    // no click remembered - show the popup here.
    function handleInstalled() {
        if (new URLSearchParams(window.location.search).get("installed") !== "1")
            return;
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
        const show = () => showInstalledPopup();
        if (document.body)
            show();
        else
            document.addEventListener("DOMContentLoaded", show);
    }
    function track(event) {
        const fbq = window.fbq;
        if (typeof fbq === "function")
            fbq("track", event);
    }
    function isChromeStoreLink(link) {
        const href = link.getAttribute("href") || "";
        return href.indexOf("chromewebstore.google.com") !== -1 || href.indexOf("chrome.google.com/webstore") !== -1;
    }
    // One listener for the whole page, so every button counts - also ones
    // added later or translated.
    document.addEventListener("click", (event) => {
        const target = event.target;
        const link = target ? target.closest("a") : null;
        if (!link || !isChromeStoreLink(link))
            return;
        // Set before the browser follows the link, so both same-tab and
        // new-tab clicks open the store with the source added.
        link.href = withSource(link.href);
        rememberInstallPage();
        track("Lead");
        // A link that opens in the same tab leaves the page at once; a short
        // wait lets the event reach Meta first. New-tab links need no wait.
        const opensNewTab = link.target === "_blank" || event.ctrlKey || event.metaKey || event.shiftKey;
        if (!opensNewTab && typeof window.fbq === "function") {
            event.preventDefault();
            const href = link.href;
            setTimeout(() => {
                window.location.href = href;
            }, 300);
        }
    });
    rememberAdSource();
    handleInstalled();
})();
