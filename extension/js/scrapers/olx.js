// scrapers/olx.js — scrapeOLX — all OLX selectors
// Returns normalized data object matching the standard shape
(function () {
  "use strict";

  /**
   * Scrapes OLX listing page and returns normalized data.
   * All selectors are OLX-specific and may need updates if OLX changes their DOM.
   *
   * @returns {Object} Normalized listing data
   */
  function scrapeOLX() {
    var data = {
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
    };

    // listing_id — extract from URL (e.g., /iid-123456789)
    var urlMatch = window.location.href.match(/iid-(\d+)/);
    data.listing_id = urlMatch ? urlMatch[1] : null;

    // title
    var titleEl = document.querySelector("h1._75bce902");
    data.title = titleEl ? titleEl.innerText.trim() : null;

    // price — strip "Rs" and commas, convert to paisas
    var priceEl = document.querySelector("span._24469da7");
    if (priceEl) {
      var priceText = priceEl.innerText.trim();
      priceText = priceText.replace(/Rs\s*/i, "").replace(/,/g, "").trim();

      if (priceText.toLowerCase().includes("crore")) {
        var num = parseFloat(priceText.replace(/crores?/i, "").trim());
        data.price = Math.round(num * 10000000);
      } else if (
        priceText.toLowerCase().includes("lac") ||
        priceText.toLowerCase().includes("lacs")
      ) {
        var num = parseFloat(priceText.replace(/lacs?/i, "").trim());
        data.price = Math.round(num * 100000);
      } else {
        data.price = Math.round(parseFloat(priceText));
      }
    }

    // description
    var descEl = document.querySelector("div._7a99ad24 span");
    data.description = descEl ? descEl.innerText.trim() : null;

    // image urls — take first 3 images
    var imageEls = document.querySelectorAll(
      "div.image-gallery-slide img._66938426",
    );
    var images = Array.from(imageEls)
      .slice(0, 3)
      .map(function (img) {
        return img.src;
      })
      .filter(function (src) {
        return src && src.includes("olx");
      });
    data.image_urls = images.length > 0 ? images : null;

    // seller name — navigate via "Posted by" label
    var postedByLabel = Array.from(
      document.querySelectorAll("span._9083bec6"),
    ).find(function (el) {
      return el.innerText.trim() === "Posted by";
    });
    if (postedByLabel && postedByLabel.nextElementSibling) {
      var nameEl = postedByLabel.nextElementSibling.querySelector("span");
      data.seller_name = nameEl ? nameEl.innerText.trim() : null;
    } else {
      data.seller_name = null;
    }

    // member since — try multiple approaches
    var memberSinceLabel = Array.from(
      document.querySelectorAll("span._9083bec6._1fcb6673"),
    ).find(function (el) {
      return el.innerText.trim() === "Member Since";
    });

    if (memberSinceLabel) {
      var yearEl = memberSinceLabel.parentElement.querySelector(
        "span._8206696c.b7af14b4",
      );
      if (!yearEl) yearEl = memberSinceLabel.nextElementSibling;
      data.seller_join_date = yearEl
        ? "Member since " + yearEl.innerText.trim()
        : null;
    } else {
      // fallback — search all text on page for "Member since YYYY" pattern
      var allText = document.body.innerText;
      var memberMatch = allText.match(/Member [Ss]ince\s+(\d{4})/);
      data.seller_join_date = memberMatch
        ? "Member since " + memberMatch[1]
        : null;
    }

    // seller profile url and platform id
    var profileLink = document.querySelector("a.da952dfc");
    if (profileLink) {
      var href = profileLink.getAttribute("href");
      data.seller_profile_url = "https://www.olx.com.pk" + href;
      var profileMatch = href.match(/\/profile\/([^\/]+)/);
      data.platform_id = profileMatch ? profileMatch[1] : null;
    }

    // location — find via SVG pin icon
    var locationSvg = document.querySelector("svg.d0356874");
    if (locationSvg && locationSvg.parentElement) {
      data.seller_location = locationSvg.parentElement.innerText.trim() || null;
    } else {
      var pinSvg = Array.from(document.querySelectorAll("svg")).find(
        function (svg) {
          return svg.getAttribute("viewBox") === "0 0 15 15";
        },
      );
      data.seller_location =
        pinSvg && pinSvg.parentElement
          ? pinSvg.parentElement.innerText.trim() || null
          : null;
    }

    var lastActiveEl = document.querySelector(
      "span[aria-label='Creation date']",
    );
    data.seller_last_active = lastActiveEl
      ? lastActiveEl.innerText.trim()
      : null;

    return data;
  }

  // Expose on global namespace
  window.__safelyScrapers = window.__safelyScrapers || {};
  window.__safelyScrapers.scrapeOLX = scrapeOLX;
})();
