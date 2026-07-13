// scrapers/index.js — detectPlatform + routes to correct scraper
(function () {
  "use strict";
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

  /**
   * Whether the CURRENT page is actually a single listing Safely can
   * read - not just "this domain is one we support at all". Being on
   * facebook.com's home feed or your own profile is a supported SITE,
   * but there is no listing there to analyze - this is the check that
   * keeps those pages from silently producing an empty "Untitled
   * listing" entry in someone's dashboard history.
   * @returns {boolean}
   */
  function isListingPage() {
    var url = window.location.href;
    var platform = detectPlatform();
    if (platform === "olx") return url.includes("iid-");
    if (platform === "facebook") return url.includes("/marketplace/item/");
    return false;
  }

  /**
   * Routes to the correct scraper based on detected platform.
   * Returns a normalized data object with identical structure regardless of platform.
   *
   * Normalized shape:
   * {
   *   listing_id: string | null,
   *   title: string | null,
   *   price: number | null,           // in paisas (smallest unit)
   *   description: string | null,
   *   image_urls: string[] | null,    // max 3 images
   *   seller_name: string | null,
   *   seller_join_date: string | null,
   *   seller_profile_url: string | null,
   *   platform_id: string | null,
   *   seller_location: string | null,
   *   seller_last_active: string | null,
   *   platform: string                // 'olx' | 'facebook'
   * }
   */
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
  // Expose on global namespace
  window.__safelyScrapers = window.__safelyScrapers || {};
  window.__safelyScrapers.detectPlatform = detectPlatform;
  window.__safelyScrapers.isListingPage = isListingPage;
  window.__safelyScrapers.scrape = scrape;
})();
