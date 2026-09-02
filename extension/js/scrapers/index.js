"use strict";
(function () {
    "use strict";
    const PROTECTED_DOMAINS = [
        { name: "OLX Pakistan", domain: "olx.com.pk" },
        { name: "Facebook", domain: "facebook.com" },
        { name: "Amazon", domain: "amazon.com" },
        { name: "eBay", domain: "ebay.com" },
    ];
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
    const platformRegistry = [
        {
            name: "olx",
            matchesHostname: (hostname) => hostname.includes("olx.com"),
            requiresClientSideScraping: true,
            isListingUrl: (url) => url.includes("iid-"),
        },
        {
            name: "b2brazil",
            matchesHostname: (hostname) => hostname.includes("b2brazil.com"),
            requiresClientSideScraping: false,
            isListingUrl: (url) => url.includes("/hotsite/"),
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
    window.__safelyScrapers = window.__safelyScrapers || {};
    window.__safelyScrapers.detectPlatform = detectPlatform;
    window.__safelyScrapers.isListingPage = isListingPage;
    window.__safelyScrapers.requiresClientSideScraping = requiresClientSideScraping;
    window.__safelyScrapers.checkDomain = checkDomain;
    window.__safelyScrapers.normalize = normalize;
    window.__safelyScrapers.editDistance = editDistance;
    window.__safelyScrapers.highlightDiff = highlightDiff;
    window.__safelyScrapers.isGenuineDomain = isGenuineDomain;
})();
