interface ProtectedDomain {
  name: string;
  domain: string;
}

interface LegitimateDomainResult {
  status: "legitimate";
  realName: string;
  realDomain: string;
}

interface SuspiciousDomainResult {
  status: "suspicious";
  realName: string;
  realDomain: string;
  currentDomain: string;
  reason: "lookalike-characters" | "similar-spelling";
  currentDomainHtml: string;
  realDomainHtml: string;
}

interface HighlightDiffResult {
  currentHtml: string;
  realHtml: string;
}

interface PlatformConfig {
  name: string;
  matchesHostname: (hostname: string) => boolean;
  requiresClientSideScraping: boolean;
  isListingUrl: (url: string) => boolean;
}

type DomainCheckResult = LegitimateDomainResult | SuspiciousDomainResult | null;

(function () {
  "use strict";

  let PROTECTED_DOMAINS: ProtectedDomain[] = [];
  const DOMAINS_CACHE_KEY = "safely_protected_domains_cache";
  const DOMAINS_CACHE_TIMESTAMP_KEY = "safely_protected_domains_cached_at";
  const CACHE_LIFETIME_MS = 24 * 60 * 60 * 1000; // 24 hours

  async function loadProtectedDomains(): Promise<void> {
    try {
      const cached = (await chrome.storage.local.get([
        DOMAINS_CACHE_KEY,
        DOMAINS_CACHE_TIMESTAMP_KEY,
      ])) as Record<string, any>;

      const cachedAt: number = cached[DOMAINS_CACHE_TIMESTAMP_KEY] || 0;
      const isFresh = Date.now() - cachedAt < CACHE_LIFETIME_MS;

      if (isFresh && cached[DOMAINS_CACHE_KEY]) {
        PROTECTED_DOMAINS = cached[DOMAINS_CACHE_KEY] as ProtectedDomain[];
        return;
      }

      const apiBase = (window as any).__safelyAPI?.SITE_BASE || "http://localhost:3000";
      const response = await fetch(apiBase + "/api/v1/platform-domains");
      const data = await response.json();
      PROTECTED_DOMAINS = Object.entries(data).map(([name, domain]) => ({
        name,
        domain: domain as string,
      }));

      await chrome.storage.local.set({
        [DOMAINS_CACHE_KEY]: PROTECTED_DOMAINS,
        [DOMAINS_CACHE_TIMESTAMP_KEY]: Date.now(),
      });
    } catch (e) {
      console.error("Safely: failed to load protected domains list", e);
    }
  }

  function normalize(hostname: string): string {
    return hostname
      .toLowerCase()
      .replace(/0/g, "o")
      .replace(/1/g, "l")
      .replace(/rn/g, "m")
      .replace(/vv/g, "w")
      .replace(/-/g, "");
  }

  function editDistance(a: string, b: string): number {
    const m = a.length;
    const n = b.length;
    const dp: number[][] = [];
    for (let i = 0; i <= m; i++) dp.push([i]);
    for (let j = 0; j <= n; j++) dp[0][j] = j;

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

  function highlightDiff(a: string, b: string): HighlightDiffResult {
    const m = a.length;
    const n = b.length;
    const dp: number[][] = [];

    for (let i = 0; i <= m; i++) {
      dp.push([]);
      for (let j = 0; j <= n; j++) {
        if (i === 0) dp[i][j] = j;
        else if (j === 0) dp[i][j] = i;
        else if (a[i - 1] === b[j - 1]) dp[i][j] = dp[i - 1][j - 1];
        else dp[i][j] = 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
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
      } else if (i2 > 0 && j2 > 0 && dp[i2][j2] === dp[i2 - 1][j2 - 1] + 1) {
        aMarks[i2 - 1] = true;
        bMarks[j2 - 1] = true;
        i2--;
        j2--;
      } else if (i2 > 0 && dp[i2][j2] === dp[i2 - 1][j2] + 1) {
        aMarks[i2 - 1] = true;
        i2--;
      } else {
        bMarks[j2 - 1] = true;
        j2--;
      }
    }

    function build(str: string, marks: boolean[]): string {
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

  function isGenuineDomain(hostname: string, realDomain: string): boolean {
    return hostname === realDomain || hostname.endsWith("." + realDomain);
  }

  function checkDomain(): DomainCheckResult {
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

  const platformRegistry: PlatformConfig[] = [
    {
      name: "olx",
      matchesHostname: (hostname) => hostname.includes("olx.com"),
      requiresClientSideScraping: false,
      isListingUrl: (url) => url.includes("iid-"),
    },
    {
      name: "b2brazil",
      matchesHostname: (hostname) => hostname.includes("b2brazil.com"),
      requiresClientSideScraping: false,
      isListingUrl: (url) => url.includes("/hotsite/"),
    },
  ];

  function detectPlatform(): string {
    const hostname = window.location.hostname;
    const match = platformRegistry.find((p) => p.matchesHostname(hostname));
    return match ? match.name : "unknown";
  }

  function requiresClientSideScraping(platform: string): boolean {
    return platformRegistry.find((p) => p.name === platform)?.requiresClientSideScraping ?? true;
  }

  function isListingPage(): boolean {
    const url = window.location.href;
    const platform = detectPlatform();
    const config = platformRegistry.find((p) => p.name === platform);
    return config ? config.isListingUrl(url) : false;
  }

  (window as any).__safelyScrapers = (window as any).__safelyScrapers || {};
  (window as any).__safelyScrapers.detectPlatform = detectPlatform;
  (window as any).__safelyScrapers.loadProtectedDomains = loadProtectedDomains;
  (window as any).__safelyScrapers.isListingPage = isListingPage;
  (window as any).__safelyScrapers.requiresClientSideScraping = requiresClientSideScraping;
  (window as any).__safelyScrapers.checkDomain = checkDomain;
  (window as any).__safelyScrapers.normalize = normalize;
  (window as any).__safelyScrapers.editDistance = editDistance;
  (window as any).__safelyScrapers.highlightDiff = highlightDiff;
  (window as any).__safelyScrapers.isGenuineDomain = isGenuineDomain;
})();
