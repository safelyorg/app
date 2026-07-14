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

  // Standard Levenshtein alignment, but instead of just returning a
  // number, this walks back through the comparison to identify EXACTLY
  // which characters differ between the two strings - a substitution,
  // an extra character, or a missing one. This is what actually solves
  // the real problem: characters like "l" and "I", or "0" and "o", are
  // deliberately designed to look near-identical in plain text, so
  // simply showing both domains side by side doesn't help someone
  // actually spot the difference - marking the specific character does.
  function highlightDiff(a, b) {
    var m = a.length,
      n = b.length;
    var dp = [];
    for (var i = 0; i <= m; i++) {
      dp.push([]);
      for (var j = 0; j <= n; j++) {
        if (i === 0) dp[i][j] = j;
        else if (j === 0) dp[i][j] = i;
        else if (a[i - 1] === b[j - 1]) dp[i][j] = dp[i - 1][j - 1];
        else {
          dp[i][j] =
            1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
        }
      }
    }

    var aMarks = new Array(m).fill(false);
    var bMarks = new Array(n).fill(false);
    var i2 = m,
      j2 = n;
    while (i2 > 0 || j2 > 0) {
      if (
        i2 > 0 &&
        j2 > 0 &&
        a[i2 - 1] === b[j2 - 1] &&
        dp[i2][j2] === dp[i2 - 1][j2 - 1]
      ) {
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

    function build(str, marks) {
      var html = "";
      for (var k = 0; k < str.length; k++) {
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
        var diffed = highlightDiff(hostname, entry.domain);
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
