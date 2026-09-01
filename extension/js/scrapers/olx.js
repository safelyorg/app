"use strict";
function extractWebsiteFromDescription(description) {
    if (!description)
        return null;
    const urlPattern = /(https?:\/\/)?(www\.)?[a-zA-Z0-9-]+\.[a-zA-Z]{2,}(\.[a-zA-Z]{2,})?/g;
    const matches = description.match(urlPattern);
    return matches?.find((m) => !m.includes("olx.com")) || null;
}
const websiteExtractors = [extractWebsiteFromDescription];
function findSellerWebsite(description) {
    for (const extractor of websiteExtractors) {
        const result = extractor(description);
        if (result)
            return result;
    }
    return null;
}
(function () {
    "use strict";
    /**
     * Scrapes OLX listing page and returns normalized data.
     * All selectors are OLX-specific and may need updates if OLX changes their DOM.
     */
    function scrapeOLX() {
        const data = {
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
            seller_website: null,
            seller_verified: false,
            seller_rating: null,
            seller_total_products: null,
        };
        // listing_id — extract from URL (e.g., /iid-123456789)
        const urlMatch = window.location.href.match(/iid-(\d+)/);
        data.listing_id = urlMatch ? urlMatch[1] : null;
        // title
        const titleEl = (document.querySelector("h1._75bce902") ||
            document.querySelector("h1.heading_h1__0cOM_"));
        data.title = titleEl ? titleEl.innerText.trim() : null;
        // price — strip "Rs" and commas, convert to paisas
        const priceEl = (document.querySelector("span._24469da7") ||
            document.querySelector('[class*="product-price_productPrice"] span:first-child'));
        if (priceEl) {
            let priceText = (priceEl.innerText || priceEl.textContent || "").trim();
            priceText = priceText.replace(/Rs\s*/i, "").replace(/,/g, "").trim();
            if (priceText.toLowerCase().includes("crore")) {
                const num = parseFloat(priceText.replace(/crores?/i, "").trim());
                data.price = Math.round(num * 10000000);
            }
            else if (priceText.toLowerCase().includes("lac") ||
                priceText.toLowerCase().includes("lacs")) {
                const num = parseFloat(priceText.replace(/lacs?/i, "").trim());
                data.price = Math.round(num * 100000);
            }
            else {
                data.price = Math.round(parseFloat(priceText));
            }
        }
        // description
        const descEl = (document.querySelector("div._7a99ad24 span") ||
            document.querySelector("#description .overview_collapsed__mve6Q"));
        data.description = descEl ? descEl.innerText.trim() : null;
        // image urls — take first 3 images
        const imageEls = document.querySelectorAll("div.image-gallery-slide img._66938426");
        const images = Array.from(imageEls)
            .slice(0, 3)
            .map((img) => img.src)
            .filter((src) => src && src.includes("olx"));
        data.image_urls = images.length > 0 ? images : null;
        // seller name — navigate via "Posted by" label
        const postedByLabel = Array.from(document.querySelectorAll("span._9083bec6")).find((el) => el.innerText.trim() === "Posted by");
        if (postedByLabel && postedByLabel.nextElementSibling) {
            const nameEl = postedByLabel.nextElementSibling.querySelector("span");
            data.seller_name = nameEl ? nameEl.innerText.trim() : null;
        }
        else {
            data.seller_name = null;
        }
        // Verified sellers use a completely different card structure ("Sold
        // by," not "Posted by"), with genuinely richer, real trust data
        // sitting directly on the listing page itself - free, Tier 1 data
        // worth capturing before ever considering a slower, Tier 2 visit to
        // the actual store page.
        const soldByLink = document.querySelector("#soldBy a");
        if (soldByLink) {
            const href = soldByLink.getAttribute("href");
            if (href) {
                data.seller_profile_url = "https://www.olx.com.pk" + href;
            }
            const nameEl = soldByLink.querySelector("h4");
            if (nameEl)
                data.seller_name = nameEl.innerText.trim();
            data.seller_verified = !!soldByLink.querySelector('[class*="verified"]');
            const statBlocks = Array.from(soldByLink.querySelectorAll('[class*="group_grid"]')).filter((el) => el.querySelector("strong"));
            for (const block of statBlocks) {
                const labelEl = block.querySelector('[class*="text-light"]');
                const valueEl = block.querySelector("strong");
                if (!labelEl || !valueEl)
                    continue;
                const label = labelEl.innerText.trim().toLowerCase();
                const value = valueEl.innerText.trim();
                if (label.includes("total products")) {
                    data.seller_total_products = parseInt(value, 10) || null;
                }
                else if (label.includes("rating")) {
                    const ratingMatch = value.match(/^([\d.]+)/);
                    data.seller_rating = ratingMatch ? parseFloat(ratingMatch[1]) : null;
                }
                else if (label.includes("member since")) {
                    // Sold-by cards show "Nov 2025" (month + year), genuinely
                    // different from Posted-by's plain year - only overwrite
                    // seller_join_date if it wasn't already found some other way.
                    if (!data.seller_join_date) {
                        data.seller_join_date = "Member since " + value;
                    }
                }
            }
        }
        // member since — try multiple approaches, but only if the
        // "Sold by" (verified seller) block above hasn't already found a
        // real join date - otherwise this always-running fallback
        // silently overwrites a correct value with null.
        if (!data.seller_join_date) {
            const memberSinceLabel = Array.from(document.querySelectorAll("span._9083bec6._1fcb6673")).find((el) => el.innerText.trim() === "Member Since");
            if (memberSinceLabel) {
                let yearEl = memberSinceLabel.parentElement?.querySelector("span._8206696c.b7af14b4");
                if (!yearEl)
                    yearEl = memberSinceLabel.nextElementSibling;
                data.seller_join_date = yearEl ? "Member since " + yearEl.innerText.trim() : null;
            }
            else {
                // fallback — search all text on page for "Member since YYYY" pattern
                const allText = document.body.innerText;
                const memberMatch = allText.match(/Member [Ss]ince\s+(\d{4})/);
                data.seller_join_date = memberMatch ? "Member since " + memberMatch[1] : null;
            }
        }
        // seller profile url and platform id
        const profileLink = document.querySelector("a.da952dfc");
        if (profileLink) {
            const href = profileLink.getAttribute("href");
            if (href) {
                data.seller_profile_url = "https://www.olx.com.pk" + href;
                const profileMatch = href.match(/\/profile\/([^\/]+)/);
                data.platform_id = profileMatch ? profileMatch[1] : null;
            }
        }
        // location — find via SVG pin icon
        const locationSvg = document.querySelector("svg.d0356874");
        if (locationSvg && locationSvg.parentElement) {
            data.seller_location = locationSvg.parentElement.innerText.trim() || null;
        }
        else {
            const pinSvg = Array.from(document.querySelectorAll("svg")).find((svg) => svg.getAttribute("viewBox") === "0 0 15 15");
            data.seller_location =
                pinSvg && pinSvg.parentElement
                    ? pinSvg.parentElement.innerText.trim() || null
                    : null;
        }
        const lastActiveEl = document.querySelector("span[aria-label='Creation date']");
        data.seller_last_active = lastActiveEl ? lastActiveEl.innerText.trim() : null;
        data.seller_website = findSellerWebsite(data.description);
        return data;
    }
    window.__safelyScrapers = window.__safelyScrapers || {};
    window.__safelyScrapers.scrapeOLX = scrapeOLX;
})();
