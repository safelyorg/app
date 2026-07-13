// api.js — all fetch calls to backend: analyze, report
(function () {
  "use strict";

  var API_BASE = "http://localhost:3000/api/v1";

  // Reads the session token that auth-bridge.js relayed from the website.
  // Returns an empty object (no Authorization header) if the person isn't
  // logged in - every existing call keeps working exactly as before for
  // anonymous use, this only adds something extra when a token exists.
  async function getAuthHeaders() {
    try {
      var result = await chrome.storage.local.get("safely_session_token");
      var token = result.safely_session_token;
      return token ? { Authorization: "Bearer " + token } : {};
    } catch (e) {
      // chrome.storage can be unavailable in rare edge cases - fail back
      // to anonymous rather than break the request entirely.
      return {};
    }
  }

  window.__safelyAPI = {
    analyze: async function (scrapedData) {
      try {
        var authHeaders = await getAuthHeaders();
        var response = await fetch(API_BASE + "/analyze", {
          method: "POST",
          headers: Object.assign(
            { "Content-Type": "application/json" },
            authHeaders,
          ),
          body: JSON.stringify(scrapedData),
        });

        var rawText = await response.text();
        if (!response.ok || rawText.startsWith("error")) {
          console.error("Safely: backend error:", rawText.substring(0, 300));
          return null;
        }

        return JSON.parse(rawText);
      } catch (error) {
        console.error(
          "Safely: failed to fetch analysis",
          error.message,
          error.stack,
        );
        return null;
      }
    },

    submitReport: async function (reportData) {
      try {
        var authHeaders = await getAuthHeaders();
        var response = await fetch(API_BASE + "/report", {
          method: "POST",
          headers: Object.assign(
            { "Content-Type": "application/json" },
            authHeaders,
          ),
          body: JSON.stringify(reportData),
        });

        var rawText = await response.text();
        if (!response.ok || rawText.startsWith("error")) {
          console.error("Safely: report error:", rawText.substring(0, 300));
          return null;
        }

        return JSON.parse(rawText);
      } catch (error) {
        console.error(
          "Safely: failed to submit report",
          error.message,
          error.stack,
        );
        return null;
      }
    },

    fetchAnalysis: async function () {
      var platform = window.__safelyScrapers.detectPlatform();
      if (platform === "unknown") return;

      // Defense-in-depth: panel.js already gates this before calling
      // fetchAnalysis at all, but checking again here means this stays
      // safe even if something else ever calls it directly - being on a
      // supported SITE isn't the same as being on an actual listing, and
      // this is what stops a plain facebook.com page from producing an
      // empty "Untitled listing" in someone's dashboard history.
      if (!window.__safelyScrapers.isListingPage()) return;

      var listing_url = window.location.href;

      // Get scraped data from the appropriate scraper
      var scraped = {};
      if (platform === "olx") {
        await new Promise(function (resolve) {
          setTimeout(resolve, 1500);
        });
        scraped = window.__safelyScrapers.scrapeOLX();
      } else if (platform === "facebook") {
        scraped = window.__safelyScrapers.scrapeFacebook();
      }

      var payload = {
        platform: platform,
        listing_url: listing_url,
        seller_id: null,
        listing_id: scraped.listing_id || null,
        title: scraped.title || null,
        price: scraped.price || null,
        description: scraped.description || null,
        category: null,
        image_urls: scraped.image_urls || null,
        posted_date: null,
        platform_id: scraped.platform_id || null,
        seller_name: scraped.seller_name || null,
        seller_handle: null,
        seller_phone: null,
        seller_profile_url: scraped.seller_profile_url || null,
        seller_join_date: scraped.seller_join_date || null,
        seller_location: scraped.seller_location || null,
        seller_last_active: scraped.seller_last_active || null,
      };

      var data = await window.__safelyAPI.analyze(payload);
      if (!data) return;

      window.__safelyData = {
        riskScore: data.risk_score,
        fraudReportCount: data.fraud_report_count,
        seller: {
          name: data.seller.name || "Unknown",
          platform: data.seller.platform || scraped.platform || "unknown",
          platformId: data.seller.platform_id || null,
          handle: data.seller.handle || "",
          accountAge: data.seller.account_age,
          verification: data.seller.verification,
          totalDeals: data.seller.total_deals,
          disputes: data.seller.disputes,
          completionRate: data.seller.completion_rate,
          location: data.seller.location || "Unknown",
          lastActive:
            scraped.seller_last_active || data.seller.last_active || "Unknown",
          networkSummary: data.seller.network_summary,
          platforms: data.seller.platforms,
          monthlyActivity: data.seller.monthly_activity,
        },
        signals: data.signals.map(function (s) {
          return {
            label: s.label,
            sub: s.sub,
            value: s.value,
            type: s.type,
          };
        }),
      };

      window.dispatchEvent(new CustomEvent("safely-data-ready"));
    },
  };
})();
