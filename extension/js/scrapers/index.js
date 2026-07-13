// scrapers/index.js — detectPlatform + routes to correct scraper +
// domain legitimacy checking
(function () {
  "use strict";

  var PROTECTED_DOMAINS = [
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
    var m = a.length,
      n = b.length;
    var dp = [];
    for (var i = 0; i <= m; i++) dp.push([i]);
    for (var j = 0; j <= n; j++) dp[0][j] = j;
    for (i = 1; i <= m; i++) {
      for (j = 1; j <= n; j++) {
        dp[i][j] =
          a[i - 1] === b[j - 1]
            ? dp[i - 1][j - 1]
            : 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
      }
    }
    return dp[m][n];
  }

  function isGenuineDomain(hostname, realDomain) {
    return hostname === realDomain || hostname.endsWith("." + realDomain);
  }

  /**
   * Checks the CURRENT page's domain against the protected marketplace
   * list. Returns one of three shapes:
   *  - { status: "legitimate", realName, realDomain } - genuinely the
   *    real site.
   *  - { status: "suspicious", realName, realDomain, currentDomain,
   *      reason } - a lookalike/typosquat of a protected site.
   *  - null - this domain isn't close to any protected site at all,
   *    nothing meaningful to report either way.
   */
  function checkDomain() {
    var hostname = window.location.hostname;

    for (var i = 0; i < PROTECTED_DOMAINS.length; i++) {
      var entry = PROTECTED_DOMAINS[i];

      if (isGenuineDomain(hostname, entry.domain)) {
        return {
          status: "legitimate",
          realName: entry.name,
          realDomain: entry.domain,
        };
      }

      var homoglyphMatch = normalize(hostname) === normalize(entry.domain);
      var distance = editDistance(hostname, entry.domain);
      var closeEnough = distance > 0 && distance <= 2 && entry.domain.length > 5;

      if (homoglyphMatch || closeEnough) {
        return {
          status: "suspicious",
          realName: entry.name,
          realDomain: entry.domain,
          currentDomain: hostname,
          reason: homoglyphMatch ? "lookalike-characters" : "similar-spelling",
        };
      }
    }

    return null;
  }

  /**
   * Detects which platform the current page belongs to.
   * @returns {string} 'olx' | 'facebook' | 'unknown'
   */
  function detectPlatform() {
    var url = window.location.href;
    if (url.includes("olx.com.pk")) return "olx";
    if (url.includes("facebook.com")) return "facebook";
    return "unknown";
  }

  function isListingPage() {
    var url = window.location.href;
    var platform = detectPlatform();
    if (platform === "olx") return url.includes("iid-");
    if (platform === "facebook") return url.includes("/marketplace/item/");
    return false;
  }

  function scrape() {
    var platform = detectPlatform();
    if (platform === "olx") {
      var data = window.__safelyScrapers.scrapeOLX();
      data.platform = "olx";
      return data;
    }
    if (platform === "facebook") {
      var data = window.__safelyScrapers.scrapeFacebook();
      data.platform = "facebook";
      return data;
    }
    return {
      listing_id: null,
      title: null,
      price: null,
      description: null,
      image_urls: null,
      seller_name: null,
      seller_join_date: null,
      seller_profile_url: null,
      platform_id: null,
      seller_location: null,
      seller_last_active: null,
      platform: "unknown",
    };
  }

  window.__safelyScrapers = window.__safelyScrapers || {};
  window.__safelyScrapers.detectPlatform = detectPlatform;
  window.__safelyScrapers.isListingPage = isListingPage;
  window.__safelyScrapers.checkDomain = checkDomain;
  window.__safelyScrapers.scrape = scrape;
})();
