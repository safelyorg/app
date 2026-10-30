"use strict";
(function () {
    "use strict";
    /// Calls Safely's own server (safely.sh, or localhost when testing)
    /// through the extension's background script, so the website the buyer
    /// is on cannot block the call (some sites, like Global Sources, block
    /// a page's requests to other servers). Works like fetch(). If the
    /// background script does not answer, the call is made from here.
    async function safelyFetch(url, init = {}) {
        const message = {
            type: "SAFELY_API_REQUEST",
            url,
            method: init.method || "GET",
            headers: init.headers || {},
            body: typeof init.body === "string" ? init.body : null,
        };
        const reply = await new Promise((resolve) => {
            try {
                chrome.runtime.sendMessage(message, (response) => {
                    // Reading lastError stops Chrome from logging "no receiver".
                    void chrome.runtime.lastError;
                    resolve(response ?? null);
                });
            }
            catch {
                resolve(null);
            }
        });
        if (!reply)
            return fetch(url, init);
        if (reply.networkError)
            throw new TypeError("Failed to fetch (" + reply.networkError + ")");
        const noBody = [101, 204, 205, 304].indexOf(reply.status) !== -1;
        return new Response(noBody ? null : reply.text, { status: reply.status });
    }
    window.__safelyFetch = safelyFetch;
    let PROTECTED_DOMAINS = [];
    const DOMAINS_CACHE_KEY = "safely_protected_domains_cache";
    const DOMAINS_CACHE_TIMESTAMP_KEY = "safely_protected_domains_cached_at";
    const CACHE_LIFETIME_MS = 24 * 60 * 60 * 1000; // 24 hours
    async function loadProtectedDomains() {
        try {
            const cached = (await chrome.storage.local.get([
                DOMAINS_CACHE_KEY,
                DOMAINS_CACHE_TIMESTAMP_KEY,
            ]));
            const cachedAt = cached[DOMAINS_CACHE_TIMESTAMP_KEY] || 0;
            const isFresh = Date.now() - cachedAt < CACHE_LIFETIME_MS;
            if (isFresh && cached[DOMAINS_CACHE_KEY]) {
                PROTECTED_DOMAINS = cached[DOMAINS_CACHE_KEY];
                return;
            }
            const apiBase = window.__safelyAPI?.SITE_BASE || "http://localhost:3000";
            const response = await safelyFetch(apiBase + "/api/v1/platform-domains");
            const data = await response.json();
            PROTECTED_DOMAINS = Object.entries(data).map(([name, domain]) => ({
                name,
                domain: domain,
            }));
            await chrome.storage.local.set({
                [DOMAINS_CACHE_KEY]: PROTECTED_DOMAINS,
                [DOMAINS_CACHE_TIMESTAMP_KEY]: Date.now(),
            });
        }
        catch (e) {
            console.error("Safely: failed to load protected domains list", e);
        }
    }
    function normalize(hostname) {
        return hostname
            .toLowerCase()
            .replace(/0/g, "o")
            .replace(/1/g, "l")
            .replace(/rn/g, "m")
            .replace(/vv/g, "w")
            .replace(/-/g, "");
    }
    function editDistance(a, b) {
        const m = a.length;
        const n = b.length;
        const dp = [];
        for (let i = 0; i <= m; i++)
            dp.push([i]);
        for (let j = 0; j <= n; j++)
            dp[0][j] = j;
        for (let i = 1; i <= m; i++) {
            for (let j = 1; j <= n; j++) {
                dp[i][j] =
                    a[i - 1] === b[j - 1]
                        ? dp[i - 1][j - 1]
                        : 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
            }
        }
        return dp[m][n];
    }
    function highlightDiff(a, b) {
        const m = a.length;
        const n = b.length;
        const dp = [];
        for (let i = 0; i <= m; i++) {
            dp.push([]);
            for (let j = 0; j <= n; j++) {
                if (i === 0)
                    dp[i][j] = j;
                else if (j === 0)
                    dp[i][j] = i;
                else if (a[i - 1] === b[j - 1])
                    dp[i][j] = dp[i - 1][j - 1];
                else
                    dp[i][j] = 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
            }
        }
        const aMarks = new Array(m).fill(false);
        const bMarks = new Array(n).fill(false);
        let i2 = m;
        let j2 = n;
        while (i2 > 0 || j2 > 0) {
            if (i2 > 0 && j2 > 0 && a[i2 - 1] === b[j2 - 1] && dp[i2][j2] === dp[i2 - 1][j2 - 1]) {
                i2--;
                j2--;
            }
            else if (i2 > 0 && j2 > 0 && dp[i2][j2] === dp[i2 - 1][j2 - 1] + 1) {
                aMarks[i2 - 1] = true;
                bMarks[j2 - 1] = true;
                i2--;
                j2--;
            }
            else if (i2 > 0 && dp[i2][j2] === dp[i2 - 1][j2] + 1) {
                aMarks[i2 - 1] = true;
                i2--;
            }
            else {
                bMarks[j2 - 1] = true;
                j2--;
            }
        }
        function build(str, marks) {
            let html = "";
            for (let k = 0; k < str.length; k++) {
                html += marks[k]
                    ? '<mark style="background:#ff5d5d33;color:#ff5d5d;font-weight:800;border-radius:3px;padding:0 2px;">' +
                        str[k] +
                        "</mark>"
                    : str[k];
            }
            return html;
        }
        return { currentHtml: build(a, aMarks), realHtml: build(b, bMarks) };
    }
    function isGenuineDomain(hostname, realDomain) {
        return hostname === realDomain || hostname.endsWith("." + realDomain);
    }
    function checkDomain() {
        const hostname = window.location.hostname;
        for (const entry of PROTECTED_DOMAINS) {
            if (isGenuineDomain(hostname, entry.domain)) {
                return {
                    status: "legitimate",
                    realName: entry.name,
                    realDomain: entry.domain,
                };
            }
            const homoglyphMatch = normalize(hostname) === normalize(entry.domain);
            const distance = editDistance(hostname, entry.domain);
            const closeEnough = distance > 0 && distance <= 2 && entry.domain.length > 5;
            if (homoglyphMatch || closeEnough) {
                const diffed = highlightDiff(hostname, entry.domain);
                return {
                    status: "suspicious",
                    realName: entry.name,
                    realDomain: entry.domain,
                    currentDomain: hostname,
                    reason: homoglyphMatch ? "lookalike-characters" : "similar-spelling",
                    currentDomainHtml: diffed.currentHtml,
                    realDomainHtml: diffed.realHtml,
                };
            }
        }
        return null;
    }
    // The last part of a B2Brazil hotsite address that is a list page,
    // not a product: /hotsite/{company}/products (and the Portuguese and
    // Spanish versions of the site).
    const B2BRAZIL_LIST_PAGES = ["products", "produtos", "productos", "services", "servicos", "servicios"];
    /// A B2Brazil product listing is /hotsite/{company}/{product}, e.g.
    /// /hotsite/daakiiya/bethel-nut. The company page (/hotsite/daakiiya),
    /// its product list (/hotsite/daakiiya/products), search, plans, blog
    /// and every other page are not listings, so they are not scanned.
    function isB2brazilListingUrl(url) {
        let parts;
        try {
            parts = new URL(url).pathname.split("/").filter(Boolean);
        }
        catch {
            return false;
        }
        return (parts.length === 3 &&
            parts[0] === "hotsite" &&
            !B2BRAZIL_LIST_PAGES.includes(parts[2].toLowerCase()));
    }
    /// A Wholesale Central page Safely scans: a product page
    /// (/products/33417104) or a supplier page (/suppliers/luxiso0001).
    /// Category, search and other list pages show many suppliers, so
    /// they are not scanned.
    function isWholesalecentralScanUrl(url) {
        let parts;
        try {
            parts = new URL(url).pathname.split("/").filter(Boolean);
        }
        catch {
            return false;
        }
        if (parts.length !== 2)
            return false;
        if (parts[0] === "products")
            return /^\d+$/.test(parts[1]);
        return parts[0] === "suppliers";
    }
    /// A Made-in-China page Safely scans: a product page
    /// ({shop}.en.made-in-china.com/product/cEvrzSKdrhpB/....html, or
    /// www.made-in-china.com/product/....html) or the supplier's own page
    /// (the home page of its shop, {shop}.en.made-in-china.com/). Search,
    /// category and other pages show many suppliers, so they are not
    /// scanned.
    function isMadeinchinaScanUrl(url) {
        let u;
        try {
            u = new URL(url);
        }
        catch {
            return false;
        }
        const host = u.hostname.toLowerCase();
        if (host !== "made-in-china.com" && !host.endsWith(".made-in-china.com"))
            return false;
        if (/^\/product\/.+\.html$/i.test(u.pathname))
            return true;
        return isMadeinchinaShopHost(host) && (u.pathname === "/" || u.pathname === "");
    }
    /// A supplier's shop: "{shop}.en.made-in-china.com" (not the main site).
    function isMadeinchinaShopHost(host) {
        return /^[a-z0-9-]+\.en\.made-in-china\.com$/.test(host) && !host.startsWith("www.");
    }
    /// A Global Sources product page Safely scans:
    /// - marketplace: www.globalsources.com/TWS-earbud/tws-earbud-1231057850p.htm
    /// - Hong Kong trade show: www.globalsources.com/products/HK/angle-rock-..._1213293180/
    /// Supplier pages (the marketplace supplier page, the trade-show
    /// exhibitor page /exhibitors/HK/...) are not scanned on their own:
    /// the scan of a product page also reads its supplier's page.
    function isGlobalsourcesScanUrl(url) {
        let u;
        try {
            u = new URL(url);
        }
        catch {
            return false;
        }
        const host = u.hostname.toLowerCase();
        if (host !== "www.globalsources.com" && host !== "globalsources.com")
            return false;
        if (isGlobalsourcesShowProductPath(u.pathname))
            return true;
        if (/^\/(products|exhibitors)\//i.test(u.pathname))
            return false;
        return /^\/[^/]+\/[^/]+-\d+p\.htm$/i.test(u.pathname);
    }
    /// A trade-show product page: /products/HK/{name}_{id}/.
    function isGlobalsourcesShowProductPath(path) {
        return /^\/products\/[a-z]{2}\/[^/]+_\d+\/?$/i.test(path);
    }
    /// A Global Sources supplier's page:
    /// "https://{shop}.manufacturer.globalsources.com/homepage_{id}.htm".
    /// The only kind of page the background script loads for a scan.
    /// Must match GLOBALSOURCES_SUPPLIER_PAGE in background.ts.
    const GLOBALSOURCES_SUPPLIER_PAGE = /^https:\/\/[a-z0-9-]+\.[a-z]+\.globalsources\.com\/homepage_\d+\.htm$/;
    /// The supplier's company profile page, next to its own page:
    /// "https://{shop}.manufacturer.globalsources.com/company-profile_{id}.htm".
    /// It has the employees, sales, contact person, hotline and email.
    /// Must match GLOBALSOURCES_PROFILE_PAGE in background.ts.
    const GLOBALSOURCES_PROFILE_PAGE = /^https:\/\/[a-z0-9-]+\.[a-z]+\.globalsources\.com\/company-profile_\d+\.htm$/;
    const platformRegistry = [
        {
            name: "b2brazil",
            matchesHostname: (hostname) => hostname.includes("b2brazil.com"),
            requiresClientSideScraping: false,
            isListingUrl: isB2brazilListingUrl,
        },
        {
            name: "alibaba",
            matchesHostname: (hostname) => hostname.includes("alibaba.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => url.includes("/product-detail/"),
        },
        {
            name: "tradewheel",
            matchesHostname: (hostname) => hostname.includes("tradewheel.com"),
            requiresClientSideScraping: true,
            isListingUrl: (url) => url.includes("/p/"),
        },
        {
            name: "exporthub",
            matchesHostname: (hostname) => hostname.includes("exporthub.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => url.includes("/product/"),
        },
        {
            name: "b2bmap",
            matchesHostname: (hostname) => hostname.includes("b2bmap.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => url.includes("/products/"),
        },
        {
            name: "kompass",
            matchesHostname: (hostname) => hostname.includes("kompass.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => {
                try {
                    return new URL(url).pathname.startsWith("/p/");
                }
                catch {
                    return false;
                }
            },
            nonListingMessage: "Safely can only check a specific product page on Kompass. Open a product listing to scan it.",
        },
        {
            name: "thomasnet",
            matchesHostname: (hostname) => hostname.includes("thomasnet.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => url.includes("/company/") && url.includes("/profile"),
        },
        {
            name: "wholesalecentral",
            matchesHostname: (hostname) => hostname.includes("wholesalecentral.com"),
            requiresClientSideScraping: false,
            isListingUrl: isWholesalecentralScanUrl,
            nonListingMessage: "Safely checks a product page or a supplier page on Wholesale Central. Open one to scan it.",
        },
        {
            name: "madeinchina",
            matchesHostname: (hostname) => hostname.includes("made-in-china.com"),
            requiresClientSideScraping: false,
            isListingUrl: isMadeinchinaScanUrl,
            nonListingMessage: "Safely checks a product page or a supplier's own page on Made-in-China. Open one to scan it.",
        },
        {
            name: "globalsources",
            matchesHostname: (hostname) => hostname === "globalsources.com" || hostname.endsWith(".globalsources.com"),
            requiresClientSideScraping: false,
            isListingUrl: isGlobalsourcesScanUrl,
            nonListingMessage: "Safely checks product pages on Global Sources, on the marketplace and on its trade-show pages. Open a product to scan it - Safely also reads the supplier's page for you.",
        },
    ];
    function detectPlatform() {
        const hostname = window.location.hostname;
        const match = platformRegistry.find((p) => p.matchesHostname(hostname));
        return match ? match.name : "unknown";
    }
    function requiresClientSideScraping(platform) {
        return platformRegistry.find((p) => p.name === platform)?.requiresClientSideScraping ?? true;
    }
    function isListingPage() {
        const url = window.location.href;
        const platform = detectPlatform();
        const config = platformRegistry.find((p) => p.name === platform);
        return config ? config.isListingUrl(url) : false;
    }
    function getUnsupportedMessage() {
        const hostname = window.location.hostname;
        const config = platformRegistry.find((p) => p.matchesHostname(hostname));
        if (config && !config.isListingUrl(window.location.href)) {
            return config.nonListingMessage || null;
        }
        return null;
    }
    window.__safelyScrapers = window.__safelyScrapers || {};
    window.__safelyScrapers.detectPlatform = detectPlatform;
    window.__safelyScrapers.loadProtectedDomains = loadProtectedDomains;
    window.__safelyScrapers.isListingPage = isListingPage;
    window.__safelyScrapers.getUnsupportedMessage = getUnsupportedMessage;
    window.__safelyScrapers.requiresClientSideScraping = requiresClientSideScraping;
    window.__safelyScrapers.checkDomain = checkDomain;
    window.__safelyScrapers.normalize = normalize;
    window.__safelyScrapers.editDistance = editDistance;
    window.__safelyScrapers.highlightDiff = highlightDiff;
    window.__safelyScrapers.isGenuineDomain = isGenuineDomain;
    window.__safelyScrapers.isB2brazilListingUrl = isB2brazilListingUrl;
    window.__safelyScrapers.isWholesalecentralScanUrl = isWholesalecentralScanUrl;
    window.__safelyScrapers.isMadeinchinaScanUrl = isMadeinchinaScanUrl;
    window.__safelyScrapers.isGlobalsourcesScanUrl = isGlobalsourcesScanUrl;
    /// Finds the real company profile link on a TradeWheel listing
    /// page, then fetches that page in the background, using the
    /// visiting user's own, real browser session (their cookies are
    /// sent automatically by the browser, since this fetch runs on the
    /// tradewheel.com origin itself). Extracts the real website field
    /// if the fetched page shows one - which only happens for a
    /// genuinely logged-in visitor.
    async function fetchTradewheelWebsite() {
        const companyLink = document.querySelector(".comp-info a");
        if (!companyLink)
            return null;
        try {
            const response = await fetch(companyLink.href, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            const doc = new DOMParser().parseFromString(html, "text/html");
            const rows = doc.querySelectorAll(".contact_details table tr");
            for (const row of Array.from(rows)) {
                const text = row.textContent?.trim() || "";
                if (text.startsWith("Website:")) {
                    const value = text.replace("Website:", "").trim();
                    if (value && value.toLowerCase() !== "show") {
                        return value;
                    }
                    return null;
                }
            }
            return null;
        }
        catch (e) {
            console.error("Safely: failed to fetch TradeWheel company page", e);
            return null;
        }
    }
    window.__safelyScrapers.fetchTradewheelWebsite = fetchTradewheelWebsite;
    /// The supplier's phone from a b2bmap product page. b2bmap masks the
    /// first (desktop) copy for visitors without a paid account
    /// ("+9203009xxxxx") but often shows the full number in the page's
    /// mobile copy of the same box. The full number is used only when it
    /// starts with the same visible digits, so a different number on the
    /// page is never taken.
    function extractB2bmapPhone() {
        const isMasked = (t) => /[x*]/i.test(t);
        const texts = Array.from(document.querySelectorAll("span.d-flex.mb-3.align-items-center span.text-muted"))
            .map((el) => (el.textContent || "").replace(/\s+/g, " ").trim())
            .filter(Boolean);
        const first = texts[0];
        if (!first)
            return null;
        if (!isMasked(first))
            return first;
        const visible = first.split(/[x*]/i)[0].trim();
        if ((visible.match(/\d/g) || []).length < 4)
            return null;
        return (texts.find((t) => !isMasked(t) && t.startsWith(visible) && t.length > visible.length) || null);
    }
    window.__safelyScrapers.extractB2bmapPhone = extractB2bmapPhone;
    /// On a Wholesale Central product page: the supplier's page, loaded
    /// in this browser (the buyer's own session, which has already passed
    /// the site's robot check), as data only (see nextDataOnlyHtml). null
    /// on a supplier page, or if the page could not be loaded.
    /// Only a page's data scripts (Next.js self.__next_f), as a small page
    /// for Safely to read. This is all the Wholesale Central scraper reads;
    /// no text, styles, tracking or login scripts are sent. null if the
    /// page has none.
    function nextDataOnlyHtml(doc) {
        const data = Array.from(doc.querySelectorAll("script"))
            .map((el) => el.textContent || "")
            .filter((text) => text.includes("self.__next_f.push"));
        if (data.length === 0)
            return null;
        return ("<!DOCTYPE html><html><body>" +
            data.map((text) => "<script>" + text + "</script>").join("") +
            "</body></html>");
    }
    /// The Wholesale Central page the buyer is on, as data only.
    function wholesalecentralPageData() {
        return nextDataOnlyHtml(document);
    }
    async function fetchWholesalecentralSupplierPage() {
        if (!window.location.pathname.startsWith("/products/"))
            return null;
        const link = document.querySelector('a[href^="/suppliers/"]');
        if (!link)
            return null;
        try {
            const response = await fetch(link.href, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            return nextDataOnlyHtml(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the Wholesale Central supplier page", e);
            return null;
        }
    }
    window.__safelyScrapers.fetchWholesalecentralSupplierPage =
        fetchWholesalecentralSupplierPage;
    window.__safelyScrapers.wholesalecentralPageData = wholesalecentralPageData;
    window.__safelyScrapers.nextDataOnlyHtml = nextDataOnlyHtml;
    /// A small copy of a Made-in-China page for Safely to read. Everything
    /// Safely reads is in the page's own HTML, so it is kept, but scripts
    /// (except the page's schema.org product and company data), styles,
    /// icons and frames are removed: no tracking or login scripts are
    /// sent. The canonical link is kept (it names the supplier's shop).
    function madeinchinaTrimmed(doc) {
        doc
            .querySelectorAll('style, link:not([rel="canonical"]), svg, iframe, noscript, video, canvas, template')
            .forEach((el) => el.remove());
        doc.querySelectorAll("script").forEach((el) => {
            if (el.getAttribute("type") !== "application/ld+json")
                el.remove();
        });
        const ownPanel = doc.querySelector("#safely-root");
        if (ownPanel)
            ownPanel.remove();
        return "<!DOCTYPE html>" + doc.documentElement.outerHTML;
    }
    /// The Made-in-China page the buyer is on, exactly as the site sends it
    /// (loaded again in this browser, so it is the same HTML Safely was
    /// tested on), trimmed. If that fails, the page as it is shown now.
    /// null only if both fail.
    async function madeinchinaPageHtml() {
        try {
            const response = await fetch(window.location.href, { credentials: "include" });
            if (response.ok) {
                const html = await response.text();
                return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
            }
        }
        catch (e) {
            console.error("Safely: could not load the Made-in-China page again", e);
        }
        try {
            return madeinchinaTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the Made-in-China page", e);
            return null;
        }
    }
    /// The supplier's shop ("https://{shop}.en.made-in-china.com") on a
    /// product page: from the page's company data, the company link, or
    /// the page's own address when it is on the shop. null if none.
    function madeinchinaShopUrl(doc) {
        for (const el of Array.from(doc.querySelectorAll('script[type="application/ld+json"]'))) {
            try {
                const data = JSON.parse(el.textContent || "");
                if (data && data["@type"] === "Organization" && typeof data.url === "string") {
                    return new URL(data.url, window.location.href).origin;
                }
            }
            catch {
                // Not valid JSON (some pages have an empty block): try the next one.
            }
        }
        const link = doc.querySelector("a.sr-comInfo-title");
        if (link && link.href) {
            try {
                return new URL(link.href, window.location.href).origin;
            }
            catch {
                // A broken link: fall through.
            }
        }
        return isMadeinchinaShopHost(window.location.hostname.toLowerCase())
            ? window.location.origin
            : null;
    }
    /// A Made-in-China supplier's own page (its shop):
    /// "https://{shop}.en.made-in-china.com/". Must match the Made-in-China
    /// line in SUPPLIER_PAGES in background.ts.
    const MADEINCHINA_SHOP_PAGE = /^https:\/\/[a-z0-9-]+\.en\.made-in-china\.com\/$/;
    /// On a Made-in-China product page: the supplier's own page, loaded in
    /// this browser, trimmed. A product page on the supplier's shop loads
    /// it itself. A product page on the main site (www.made-in-china.com)
    /// cannot load another address, so the background script loads it
    /// (only that kind of page - see background.ts), without the buyer's
    /// cookies (tested 2026-10-29: the real page comes back). null on a
    /// supplier page, or if it could not be loaded; then Safely's server
    /// loads it through ScraperAPI.
    async function fetchMadeinchinaSupplierPage() {
        if (!window.location.pathname.startsWith("/product/"))
            return null;
        const shop = madeinchinaShopUrl(document);
        if (!shop)
            return null;
        const url = shop.toLowerCase() + "/";
        if (shop === window.location.origin) {
            try {
                const response = await fetch(url, { credentials: "include" });
                if (!response.ok)
                    return null;
                const html = await response.text();
                return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
            }
            catch (e) {
                console.error("Safely: failed to load the Made-in-China supplier page", e);
                return null;
            }
        }
        if (!MADEINCHINA_SHOP_PAGE.test(url))
            return null;
        const response = await askBackground({ type: "SAFELY_FETCH_SUPPLIER_PAGE", url }, 20000);
        if (!response || typeof response.html !== "string" || response.html.length === 0) {
            console.error("Safely: the Made-in-China supplier page could not be loaded in the browser");
            return null;
        }
        try {
            return madeinchinaTrimmed(new DOMParser().parseFromString(response.html, "text/html"));
        }
        catch (e) {
            console.error("Safely: could not read the Made-in-China supplier page", e);
            return null;
        }
    }
    window.__safelyScrapers.madeinchinaPageHtml = madeinchinaPageHtml;
    window.__safelyScrapers.fetchMadeinchinaSupplierPage = fetchMadeinchinaSupplierPage;
    window.__safelyScrapers.madeinchinaTrimmed = madeinchinaTrimmed;
    /// The supplier's contact and registration details that Global Sources
    /// keeps in its page data ("contactUsInfo": address, own website,
    /// official registration), which the trimming would remove with the
    /// scripts. Only that one small block is kept - no other script.
    function globalsourcesContactData(rawHtml) {
        const match = rawHtml.match(/contactUsInfo:\{[^{}]*\}/);
        if (!match)
            return "";
        const safe = match[0].replace(/<\//g, "<\\/");
        return '<script type="text/x-safely-data">' + safe + "</script>";
    }
    /// One value from Nuxt's page data (the "__NUXT_DATA__" script of the
    /// trade-show pages): a flat list where every object field and list
    /// item is the number of another entry; ["Reactive", n] and the like
    /// wrap a value.
    function nuxtValue(items, index, depth) {
        if (typeof index !== "number" || index < 0 || depth > 60)
            return index ?? null;
        const item = items[index];
        if (Array.isArray(item)) {
            if (typeof item[0] === "string")
                return item.length > 1 ? nuxtValue(items, item[1], depth + 1) : null;
            return item.map((v) => nuxtValue(items, v, depth + 1));
        }
        if (item && typeof item === "object") {
            const out = {};
            for (const key of Object.keys(item))
                out[key] = nuxtValue(items, item[key], depth + 1);
            return out;
        }
        return item;
    }
    /// The trade-show page's own data: the product and supplier
    /// ("exhibitorProductDetail") or the supplier and its products with
    /// prices ("ExhibitorDetail"). null on other pages.
    function globalsourcesShowData(doc) {
        const script = doc.querySelector("script#__NUXT_DATA__");
        if (!script)
            return null;
        try {
            const items = JSON.parse(script.textContent || "");
            const data = nuxtValue(items, 0, 0)?.data || {};
            const out = {};
            if (data.exhibitorProductDetail)
                out.exhibitorProductDetail = data.exhibitorProductDetail;
            if (data.ExhibitorDetail)
                out.ExhibitorDetail = data.ExhibitorDetail;
            return Object.keys(out).length ? out : null;
        }
        catch {
            return null;
        }
    }
    /// Nuxt's page data format again, for only the parts Safely reads, so
    /// nothing else from the page data (the visitor's own account, for
    /// example) is sent.
    function nuxtDataScript(data) {
        const items = [["ShallowReactive", 1]];
        const add = (v) => {
            const at = items.length;
            items.push(null);
            if (Array.isArray(v))
                items[at] = v.map(add);
            else if (v && typeof v === "object") {
                const fields = {};
                for (const key of Object.keys(v))
                    fields[key] = add(v[key]);
                items[at] = fields;
            }
            else
                items[at] = v ?? null;
            return at;
        };
        add({ data });
        const json = JSON.stringify(items).replace(/</g, "\\u003c");
        return '<script type="application/json" id="__NUXT_DATA__">' + json + "</script>";
    }
    /// A trimmed Global Sources page plus the small data blocks Safely
    /// reads: the contact block (marketplace) or the product and supplier
    /// data (trade show).
    function globalsourcesTrimmed(rawHtml, doc) {
        const show = globalsourcesShowData(doc);
        const trimmed = madeinchinaTrimmed(doc);
        const data = globalsourcesContactData(rawHtml) + (show ? nuxtDataScript(show) : "");
        if (!data)
            return trimmed;
        const end = trimmed.lastIndexOf("</body>");
        return end === -1 ? trimmed + data : trimmed.slice(0, end) + data + trimmed.slice(end);
    }
    /// "Anhui Dujia Office Supplies Manufacturing Co.,Ltd" ->
    /// "anhui-dujia-office-supplies-manufacturing-co-ltd". Must match slug
    /// in backend globalsources/show.rs.
    function globalsourcesSlug(name) {
        return name
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, "-")
            .replace(/^-+|-+$/g, "");
    }
    /// On a trade-show product page: the supplier's exhibitor page,
    /// "https://www.globalsources.com/exhibitors/HK/{company}_{supplier id}/".
    function globalsourcesExhibitorUrl(doc, pageUrl) {
        const supplier = globalsourcesShowData(doc)?.exhibitorProductDetail?.data?.supplierInfo;
        if (!supplier || !supplier.supplierId || !supplier.companyName)
            return null;
        const show = (new URL(pageUrl).pathname.match(/^\/products\/([a-z]{2})\//i) || [])[1] || "HK";
        return ("https://www.globalsources.com/exhibitors/" +
            show.toUpperCase() +
            "/" +
            globalsourcesSlug(String(supplier.companyName)) +
            "_" +
            String(supplier.supplierId) +
            "/");
    }
    /// The Global Sources product page the buyer is on, exactly as the site
    /// sends it (loaded again in this browser), trimmed the same way as a
    /// Made-in-China page (no scripts except the page's schema.org data,
    /// no styles). If that fails, the page as it is shown now. null only if
    /// both fail.
    async function globalsourcesPageHtml() {
        try {
            const response = await fetch(window.location.href, { credentials: "include" });
            if (response.ok) {
                const html = await response.text();
                return globalsourcesTrimmed(html, new DOMParser().parseFromString(html, "text/html"));
            }
        }
        catch (e) {
            console.error("Safely: could not load the Global Sources page again", e);
        }
        try {
            return globalsourcesTrimmed(document.documentElement.outerHTML, document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the Global Sources page", e);
            return null;
        }
    }
    /// The supplier's page linked from a Global Sources product page
    /// ("//boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm"
    /// -> "https://boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm").
    /// The link in the supplier box first, then any link to a supplier
    /// page. null if none.
    function globalsourcesSupplierUrl(doc) {
        const links = [
            ...Array.from(doc.querySelectorAll('.company-info a[href*="/homepage_"]')),
            ...Array.from(doc.querySelectorAll('a[href*="/homepage_"]')),
        ];
        for (const link of links) {
            const href = (link.getAttribute("href") || "").trim();
            let url;
            try {
                url = new URL(href.startsWith("//") ? "https:" + href : href, "https://www.globalsources.com/").href;
            }
            catch {
                continue;
            }
            url = url.split(/[?#]/)[0];
            if (GLOBALSOURCES_SUPPLIER_PAGE.test(url))
                return url;
        }
        return null;
    }
    /// Waits for the background script's answer, but not forever.
    function askBackground(message, timeoutMs) {
        return new Promise((resolve) => {
            const timer = setTimeout(() => resolve(null), timeoutMs);
            try {
                chrome.runtime.sendMessage(message, (response) => {
                    clearTimeout(timer);
                    // Reading lastError stops Chrome from logging "no receiver".
                    void chrome.runtime.lastError;
                    resolve(response ?? null);
                });
            }
            catch {
                clearTimeout(timer);
                resolve(null);
            }
        });
    }
    /// Loads one Global Sources supplier page through the background script
    /// (it is on another address, {shop}.manufacturer.globalsources.com,
    /// which a product page cannot load itself), trimmed. null if it could
    /// not be loaded.
    async function loadGlobalsourcesSupplierPage(url) {
        const response = await askBackground({ type: "SAFELY_FETCH_SUPPLIER_PAGE", url }, 20000);
        if (!response || typeof response.html !== "string" || response.html.length === 0) {
            console.error("Safely: the Global Sources page could not be loaded in the browser: " + url);
            return null;
        }
        try {
            return globalsourcesTrimmed(response.html, new DOMParser().parseFromString(response.html, "text/html"));
        }
        catch (e) {
            console.error("Safely: could not read the Global Sources page " + url, e);
            return null;
        }
    }
    /// On a Global Sources product page: the supplier's pages, trimmed.
    /// - Marketplace: the supplier's own page and its company profile page
    ///   (the second page: employees, sales, contact person, hotline and
    ///   email), both loaded by the background script (only those kinds of
    ///   page - see background.ts).
    /// - Trade show: the exhibitor page, on this same address, so it is
    ///   loaded from here; there is no second page.
    /// Either is null if it could not be loaded; then Safely's server loads
    /// that one through ScraperAPI.
    async function fetchGlobalsourcesSupplierPages() {
        const none = { company: null, extended: null };
        if (!isGlobalsourcesScanUrl(window.location.href))
            return none;
        if (isGlobalsourcesShowProductPath(window.location.pathname)) {
            const exhibitor = globalsourcesExhibitorUrl(document, window.location.href);
            if (!exhibitor)
                return none;
            try {
                const response = await fetch(exhibitor, { credentials: "include" });
                if (!response.ok)
                    return none;
                const html = await response.text();
                return {
                    company: globalsourcesTrimmed(html, new DOMParser().parseFromString(html, "text/html")),
                    extended: null,
                };
            }
            catch (e) {
                console.error("Safely: failed to load the Global Sources exhibitor page", e);
                return none;
            }
        }
        const url = globalsourcesSupplierUrl(document);
        if (!url)
            return none;
        const profile = url.replace("/homepage_", "/company-profile_");
        const [company, extended] = await Promise.all([
            loadGlobalsourcesSupplierPage(url),
            GLOBALSOURCES_PROFILE_PAGE.test(profile)
                ? loadGlobalsourcesSupplierPage(profile)
                : Promise.resolve(null),
        ]);
        return { company, extended };
    }
    /// An Alibaba company's own page, the only kind of Alibaba page the
    /// background script loads for a scan:
    /// "https://{shop}.en.alibaba.com/company_profile.html".
    /// Must match the Alibaba line in SUPPLIER_PAGES in background.ts.
    const ALIBABA_COMPANY_PAGE = /^https:\/\/[a-z0-9-]+\.en\.alibaba\.com\/company_profile\.html$/;
    /// "https://ZL9809.m.en.alibaba.com/x?y" or "zl9809.en.alibaba.com"
    /// -> "https://zl9809.en.alibaba.com/company_profile.html". null when
    /// it is not a company shop address. Same rule as normalize_shop_host
    /// in backend alibaba.rs.
    function alibabaCompanyPageFrom(raw) {
        let host;
        try {
            const text = raw.trim();
            const withScheme = /^https?:\/\//i.test(text) ? text : "https://" + text.replace(/^\/\//, "");
            host = new URL(withScheme).hostname.toLowerCase();
        }
        catch {
            return null;
        }
        host = host.replace(/^www\./, "").replace(/\.m\.en\.alibaba\.com$/, ".en.alibaba.com");
        const url = "https://" + host + "/company_profile.html";
        return ALIBABA_COMPANY_PAGE.test(url) ? url : null;
    }
    /// The company page of the supplier on an Alibaba product page. Read
    /// the same way Safely's server reads it: the "Company profile" link
    /// first, then the shop address in the page's own product data
    /// ("subDomain"). null if neither is there.
    function alibabaCompanyUrl(doc) {
        for (const link of Array.from(doc.querySelectorAll("a[href]"))) {
            if ((link.textContent || "").trim() !== "Company profile")
                continue;
            const url = alibabaCompanyPageFrom(link.getAttribute("href") || "");
            if (url)
                return url;
        }
        for (const script of Array.from(doc.querySelectorAll("script"))) {
            const match = (script.textContent || "").match(/"subDomain"\s*:\s*"([^"]+)"/);
            if (match) {
                const url = alibabaCompanyPageFrom(match[1]);
                if (url)
                    return url;
            }
        }
        return null;
    }
    /// On an Alibaba product page: the company's own page, trimmed. It is
    /// on another address ({shop}.en.alibaba.com), which this page cannot
    /// load itself, so the background script loads it (only that kind of
    /// page - see background.ts), without the buyer's cookies. Tested
    /// 2026-10-29: Alibaba sends the real page this way (no robot check),
    /// with the company data in its module-data parts. null if it could
    /// not be loaded; then Safely's server loads it through ScraperAPI.
    async function fetchAlibabaCompanyPage() {
        if (!/\/product-detail\//.test(window.location.pathname))
            return null;
        const url = alibabaCompanyUrl(document);
        if (!url)
            return null;
        const response = await askBackground({ type: "SAFELY_FETCH_SUPPLIER_PAGE", url }, 20000);
        if (!response || typeof response.html !== "string" || response.html.length === 0) {
            console.error("Safely: the Alibaba company page could not be loaded in the browser");
            return null;
        }
        try {
            return madeinchinaTrimmed(new DOMParser().parseFromString(response.html, "text/html"));
        }
        catch (e) {
            console.error("Safely: could not read the Alibaba company page", e);
            return null;
        }
    }
    /// A Kompass company page: "https://www.kompass.com/c/{name}/{id}/"
    /// (other country sites like fr.kompass.com too).
    const KOMPASS_COMPANY_PAGE = /^https:\/\/[a-z]+\.kompass\.com\/c\/[^/?#]+\/[a-z0-9]+\/$/;
    /// Kompass's robot check (DataDome). Must match the Kompass entry in
    /// BLOCK_PAGE_MARKERS in backend/services/sources/b2b/mod.rs.
    const KOMPASS_ROBOT_CHECK = "Please enable JS and disable any ad blocker";
    /// On a Kompass product page: the page as it is shown in this tab,
    /// trimmed. It is copied, not loaded again: Kompass shows its robot
    /// check to a page loaded again without the buyer's cookies (tested
    /// 2026-10-29), but this tab has already passed it. null if the tab
    /// shows the robot check itself.
    function kompassPageHtml() {
        try {
            const copy = document.cloneNode(true);
            const html = madeinchinaTrimmed(copy);
            return html.includes(KOMPASS_ROBOT_CHECK) ? null : html;
        }
        catch (e) {
            console.error("Safely: could not copy the Kompass page", e);
            return null;
        }
    }
    /// The company page linked from a Kompass product page ("See the
    /// company", the same link Safely's server reads). null if none.
    function kompassCompanyUrl(doc) {
        const link = doc.querySelector("nav#productNav a[title='See the company']");
        const href = link ? (link.getAttribute("href") || "").trim() : "";
        if (!href)
            return null;
        let url;
        try {
            url = new URL(href, "https://www.kompass.com/").href.split(/[?#]/)[0];
        }
        catch {
            return null;
        }
        return KOMPASS_COMPANY_PAGE.test(url) ? url : null;
    }
    /// On a Kompass product page: the company page, trimmed. It is on the
    /// same site, so it is loaded from here with the buyer's cookies -
    /// the one that says this browser passed the robot check is needed
    /// (without cookies Kompass shows the robot check, tested 2026-10-29).
    /// null if it could not be loaded or shows the robot check; then
    /// Safely's server loads it through ScraperAPI.
    async function fetchKompassCompanyPage() {
        if (!window.location.pathname.startsWith("/p/"))
            return null;
        const url = kompassCompanyUrl(document);
        if (!url)
            return null;
        try {
            const response = await fetch(url, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            if (html.includes(KOMPASS_ROBOT_CHECK)) {
                console.error("Safely: Kompass showed its robot check for the company page");
                return null;
            }
            return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the Kompass company page", e);
            return null;
        }
    }
    /// An ExportHub company page: "https://www.exporthub.com/{company}/".
    /// Its second page is the same address + "profile.html".
    const EXPORTHUB_COMPANY_PAGE = /^https:\/\/www\.exporthub\.com\/[a-z0-9-]+\/$/;
    /// On an ExportHub product page: the page as it is shown in this tab,
    /// trimmed. ExportHub pages also load fine again without cookies
    /// (tested 2026-10-29), but copying needs no request at all.
    function exporthubPageHtml() {
        try {
            return madeinchinaTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the ExportHub page", e);
            return null;
        }
    }
    /// The company page linked from an ExportHub product page (the
    /// company box in the sidebar, the same link Safely's server reads).
    /// null if none.
    function exporthubCompanyUrl(doc) {
        const link = doc.querySelector(".product-del_sidebar__comp-nm a");
        const href = link ? (link.getAttribute("href") || "").trim() : "";
        if (!href)
            return null;
        let url;
        try {
            url = new URL(href, "https://www.exporthub.com/").href.split(/[?#]/)[0];
        }
        catch {
            return null;
        }
        if (!url.endsWith("/"))
            url += "/";
        return EXPORTHUB_COMPANY_PAGE.test(url) ? url : null;
    }
    /// Loads one ExportHub page from this same site, trimmed. null if it
    /// could not be loaded.
    async function loadExporthubPage(url) {
        try {
            const response = await fetch(url, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the ExportHub page " + url, e);
            return null;
        }
    }
    /// On an ExportHub product page: the company page and its second page
    /// ("profile.html"), both trimmed. Either is null if it could not be
    /// loaded; then Safely's server loads that one through ScraperAPI.
    async function fetchExporthubCompanyPages() {
        const none = { company: null, extended: null };
        if (!window.location.pathname.startsWith("/product/"))
            return none;
        const url = exporthubCompanyUrl(document);
        if (!url)
            return none;
        const [company, extended] = await Promise.all([
            loadExporthubPage(url),
            loadExporthubPage(url + "profile.html"),
        ]);
        return { company, extended };
    }
    /// ThomasNet's profile page is also the company page, so it is the
    /// only page sent. Its data is in the page's Next.js data script
    /// (script#__NEXT_DATA__), which is kept; every other script is
    /// removed. ThomasNet pages load fine again without cookies (tested
    /// 2026-10-29).
    function thomasnetTrimmed(doc) {
        const nextData = doc.querySelector("script#__NEXT_DATA__");
        const kept = nextData ? nextData.outerHTML : "";
        const html = madeinchinaTrimmed(doc);
        if (!kept)
            return html;
        const end = html.lastIndexOf("</body>");
        return end === -1 ? html + kept : html.slice(0, end) + kept + html.slice(end);
    }
    /// The company part of a ThomasNet profile address,
    /// "/company/applied-rapid-technologies-1095055/profile" ->
    /// "applied-rapid-technologies-1095055". null if there is none.
    function thomasnetSlug(url) {
        const match = url.match(/\/company\/([^/?#]+)/);
        return match ? match[1].toLowerCase() : null;
    }
    /// The ThomasNet profile page the buyer is on, trimmed. It is loaded
    /// again: ThomasNet moves between pages without reloading, and then
    /// the data script still belongs to the first page opened. If loading
    /// fails, the tab is copied, but only when its data is for this same
    /// company. null if neither works; then Safely's server loads it
    /// through ScraperAPI.
    async function thomasnetPageHtml() {
        const slug = thomasnetSlug(window.location.pathname);
        if (!slug)
            return null;
        try {
            const response = await fetch(window.location.href, { credentials: "include" });
            if (response.ok) {
                const html = await response.text();
                return thomasnetTrimmed(new DOMParser().parseFromString(html, "text/html"));
            }
        }
        catch (e) {
            console.error("Safely: could not load the ThomasNet page again", e);
        }
        try {
            const canonical = document.querySelector('link[rel="canonical"]');
            const nextData = document.querySelector("script#__NEXT_DATA__")?.textContent || "";
            const sameCompany = (canonical && thomasnetSlug(canonical.href) === slug) ||
                nextData.toLowerCase().includes("/company/" + slug + "/");
            if (!sameCompany)
                return null;
            return thomasnetTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the ThomasNet page", e);
            return null;
        }
    }
    /// On a B2Brazil product page (/hotsite/{company}/{product}): the
    /// page as it is shown in this tab, trimmed. B2Brazil pages also load
    /// fine again without cookies (tested 2026-10-29), but copying needs
    /// no request at all.
    function b2brazilPageHtml() {
        try {
            return madeinchinaTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the B2Brazil page", e);
            return null;
        }
    }
    /// The company's page, "https://b2brazil.com/hotsite/{company}". Taken
    /// from the address of the product page the buyer is on - the same
    /// /hotsite/{company} part Safely's server uses. null when the page
    /// is not a product page.
    function b2brazilCompanyUrl(pageUrl) {
        let url;
        try {
            url = new URL(pageUrl);
        }
        catch {
            return null;
        }
        if (!isB2brazilListingUrl(url.href))
            return null;
        const company = url.pathname.split("/").filter(Boolean)[1];
        return /^[A-Za-z0-9_-]+$/.test(company) ? url.origin + "/hotsite/" + company : null;
    }
    /// On a B2Brazil product page: the company's page, loaded from this
    /// same site and trimmed. null if it could not be loaded; then
    /// Safely's server loads it through ScraperAPI.
    async function fetchB2brazilCompanyPage() {
        const url = b2brazilCompanyUrl(window.location.href);
        if (!url)
            return null;
        try {
            const response = await fetch(url, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the B2Brazil company page", e);
            return null;
        }
    }
    /// On a TradeWheel product page (/p/...): the page as it is shown in
    /// this tab, trimmed. TradeWheel pages also load fine again without
    /// cookies (tested 2026-10-29), but copying needs no request at all.
    function tradewheelPageHtml() {
        try {
            return madeinchinaTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the TradeWheel page", e);
            return null;
        }
    }
    /// The company page linked from a TradeWheel product page (the company
    /// box, the same link Safely's server reads):
    /// "https://www.tradewheel.com/co/{company}/". null if none.
    function tradewheelCompanyUrl(doc) {
        for (const link of Array.from(doc.querySelectorAll(".comp-info a[href]"))) {
            const href = (link.getAttribute("href") || "").trim();
            if (!href.includes("/co/"))
                continue;
            try {
                const url = new URL(href, "https://www.tradewheel.com/");
                const match = url.pathname.match(/^\/co\/([a-z0-9-]+)\/?$/i);
                if (url.hostname === "www.tradewheel.com" && match) {
                    return "https://www.tradewheel.com/co/" + match[1] + "/";
                }
            }
            catch {
                // not a usable link
            }
        }
        return null;
    }
    /// Loads one TradeWheel page from this same site, trimmed. null if it
    /// could not be loaded.
    async function loadTradewheelPage(url) {
        try {
            const response = await fetch(url, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the TradeWheel page " + url, e);
            return null;
        }
    }
    /// On a TradeWheel product page: the company page and its About Us
    /// page ("/co/{company}/about-us/"), both loaded from this same site and
    /// trimmed. The company page has the employees and the contact person;
    /// About Us has the website, the factory details and the company's own
    /// text. Either is null if it could not be loaded; then Safely's server
    /// loads that one through ScraperAPI.
    async function fetchTradewheelCompanyPages() {
        const none = { company: null, extended: null };
        if (!window.location.pathname.startsWith("/p/"))
            return none;
        const url = tradewheelCompanyUrl(document);
        if (!url)
            return none;
        const [company, extended] = await Promise.all([
            loadTradewheelPage(url),
            loadTradewheelPage(url + "about-us/"),
        ]);
        return { company, extended };
    }
    /// b2bmap's own sections - never a company page. Same list as
    /// company_slug in backend b2bmap.rs.
    const B2BMAP_SECTIONS = [
        "products",
        "product-list",
        "companies",
        "company-list",
        "buyleads",
        "business-directory",
        "pricing",
        "myzone",
        "contact-info",
    ];
    /// On a B2BMap product page (/products/...): the page as it is shown
    /// in this tab, trimmed. B2BMap pages also load fine again without
    /// cookies (tested 2026-10-29), but copying needs no request at all.
    function b2bmapPageHtml() {
        try {
            return madeinchinaTrimmed(document.cloneNode(true));
        }
        catch (e) {
            console.error("Safely: could not copy the B2BMap page", e);
            return null;
        }
    }
    /// The company page linked from a B2BMap product page (the supplier
    /// name, the same link Safely's server reads):
    /// "https://b2bmap.com/{company}". null if none.
    function b2bmapCompanyUrl(doc) {
        const link = doc.querySelector("h4.text-18.text-lg-22 a[href]");
        const href = link ? (link.getAttribute("href") || "").trim() : "";
        if (!href)
            return null;
        try {
            const url = new URL(href, "https://b2bmap.com/");
            if (url.hostname !== "b2bmap.com" && url.hostname !== "www.b2bmap.com")
                return null;
            const slug = url.pathname.split("/").filter(Boolean)[0];
            if (!slug || !/^[a-z0-9-]+$/i.test(slug))
                return null;
            if (B2BMAP_SECTIONS.includes(slug.toLowerCase()))
                return null;
            return "https://b2bmap.com/" + slug.toLowerCase();
        }
        catch {
            return null;
        }
    }
    /// On a B2BMap product page: the company page, loaded from this same
    /// site and trimmed. null if it could not be loaded; then Safely's
    /// server loads it through ScraperAPI.
    async function fetchB2bmapCompanyPage() {
        if (!window.location.pathname.startsWith("/products/"))
            return null;
        const url = b2bmapCompanyUrl(document);
        if (!url)
            return null;
        try {
            const response = await fetch(url, { credentials: "include" });
            if (!response.ok)
                return null;
            const html = await response.text();
            return madeinchinaTrimmed(new DOMParser().parseFromString(html, "text/html"));
        }
        catch (e) {
            console.error("Safely: failed to load the B2BMap company page", e);
            return null;
        }
    }
    window.__safelyScrapers.b2bmapPageHtml = b2bmapPageHtml;
    window.__safelyScrapers.b2bmapCompanyUrl = b2bmapCompanyUrl;
    window.__safelyScrapers.fetchB2bmapCompanyPage = fetchB2bmapCompanyPage;
    window.__safelyScrapers.tradewheelPageHtml = tradewheelPageHtml;
    window.__safelyScrapers.tradewheelCompanyUrl = tradewheelCompanyUrl;
    window.__safelyScrapers.fetchTradewheelCompanyPages = fetchTradewheelCompanyPages;
    window.__safelyScrapers.b2brazilPageHtml = b2brazilPageHtml;
    window.__safelyScrapers.b2brazilCompanyUrl = b2brazilCompanyUrl;
    window.__safelyScrapers.fetchB2brazilCompanyPage = fetchB2brazilCompanyPage;
    window.__safelyScrapers.thomasnetPageHtml = thomasnetPageHtml;
    window.__safelyScrapers.thomasnetTrimmed = thomasnetTrimmed;
    window.__safelyScrapers.exporthubPageHtml = exporthubPageHtml;
    window.__safelyScrapers.exporthubCompanyUrl = exporthubCompanyUrl;
    window.__safelyScrapers.fetchExporthubCompanyPages = fetchExporthubCompanyPages;
    window.__safelyScrapers.kompassPageHtml = kompassPageHtml;
    window.__safelyScrapers.kompassCompanyUrl = kompassCompanyUrl;
    window.__safelyScrapers.fetchKompassCompanyPage = fetchKompassCompanyPage;
    window.__safelyScrapers.alibabaCompanyUrl = alibabaCompanyUrl;
    window.__safelyScrapers.fetchAlibabaCompanyPage = fetchAlibabaCompanyPage;
    window.__safelyScrapers.globalsourcesPageHtml = globalsourcesPageHtml;
    window.__safelyScrapers.globalsourcesSupplierUrl = globalsourcesSupplierUrl;
    window.__safelyScrapers.globalsourcesTrimmed = globalsourcesTrimmed;
    window.__safelyScrapers.globalsourcesExhibitorUrl = globalsourcesExhibitorUrl;
    window.__safelyScrapers.fetchGlobalsourcesSupplierPages = fetchGlobalsourcesSupplierPages;
})();
